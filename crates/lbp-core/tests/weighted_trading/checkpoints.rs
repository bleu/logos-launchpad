use super::fixture::{
    checkpoint_events, config, funded, id, operation_at, prepared, state_of, weight,
};
use lbp_core::{Message, compute_pool_pda, handle};
use lee_core::account::{AccountId, Actor};
use lee_core::program::{PdaSeed, ReceiveInput};
use weighted_math_core::{calc_in_given_out, interpolate_weight};

fn poke_at(
    timestamp_ms: u64,
    input: &ReceiveInput,
    state: &lbp_core::PoolAccount,
) -> Result<lee_core::program::Response, lbp_core::Error> {
    let mut input = input.clone();
    input.pre_state = borsh::to_vec(state).unwrap().into();
    input.message = borsh::to_vec(&Message::PokeWeighted).unwrap();
    // Anyone can send poke — use a random sender
    input.from = Some(Actor::new(id(99), id(8)));
    let response = handle(&input)?;
    let source = response.sync_messages.last().unwrap().to;
    assert_eq!(source, lbp_core::pricing_clock::actor());
    let input = ReceiveInput {
        receiver: input.receiver,
        from: Some(source),
        is_authorized: false,
        pre_state: response.post_state.unwrap(),
        message: borsh::to_vec(&clock_core::Reply::Timestamp(timestamp_ms)).unwrap(),
    };
    handle(&input)
}

fn checkpoint(state: &lbp_core::PoolAccount) -> (u64, u128) {
    let lbp_core::PoolMode::Weighted {
        checkpoint_time,
        checkpoint_token_weight,
        ..
    } = state.mode
    else {
        panic!("expected weighted state");
    };
    (checkpoint_time, checkpoint_token_weight)
}

fn assert_checkpoint_events_match(
    actual: &lee_core::program::Response,
    expected: &lee_core::program::Response,
) {
    let actual = checkpoint_events(actual);
    let expected = checkpoint_events(expected);
    assert_eq!(actual.len(), 1);
    assert_eq!(expected.len(), 1);
    assert_eq!(actual[0].selector, expected[0].selector);
    assert_eq!(actual[0].data, expected[0].data);
}

#[test]
fn poke_is_permissionless_and_records_checkpoint() {
    let (input, state) = funded();
    assert_eq!(checkpoint(&state), (0, 0));
    let response = poke_at(1_500_000, &input, &state).unwrap();
    let after = state_of(&response);
    let (time, token_w) = checkpoint(&after);
    assert_eq!(time, 1500);
    // At midpoint of 1000..3000, token weight interpolates between 95% and 50%
    let expected = interpolate_weight(weight(95), weight(50), 1000, 3000, 1500);
    assert_eq!(token_w, expected);
    assert_eq!(checkpoint_events(&response).len(), 1);
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let hash = Impl::hash_bytes(b"lbp::WeightedCheckpoint");
    assert_eq!(
        checkpoint_events(&response)[0].selector,
        hash.as_bytes()[..8]
    );
    let mut payload = 1500u64.to_le_bytes().to_vec();
    payload.extend_from_slice(&expected.to_le_bytes());
    assert_eq!(checkpoint_events(&response)[0].data, payload);
}

#[test]
fn poke_authenticates_upstream_replies_and_rejects_replayed_replies() {
    let (mut input, state) = funded();
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::PokeWeighted).unwrap();
    let pending = handle(&input).unwrap();
    assert_eq!(pending.sync_messages.len(), 1);
    assert_eq!(
        pending.sync_messages[0].message,
        borsh::to_vec(&clock_core::Message::ReadTimestamp { granularity: 1 }).unwrap()
    );
    input.pre_state = pending.post_state.unwrap();
    input.from = Some(clock_core::clock_actor());
    input.message = borsh::to_vec(&clock_core::Reply::Timestamp(1_500_999)).unwrap();
    let valid = input.clone();
    let clock = clock_core::clock_actor();
    for sender in [
        None,
        Some(Actor::new(id(99), clock.program_account_id)),
        Some(Actor::new(clock.account_id, id(99))),
    ] {
        input.from = sender;
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
    }
    for bytes in [
        vec![],
        vec![255],
        borsh::to_vec(&clock_core::Reply::BlockId(1500)).unwrap(),
    ] {
        input = valid.clone();
        input.message = bytes;
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
    }
    let completed = handle(&valid).unwrap();
    assert_eq!(checkpoint(&state_of(&completed)).0, 1500);
    input = valid;
    input.pre_state = completed.post_state.unwrap();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn poke_at_start_records_start_weight() {
    let (input, state) = funded();
    let response = poke_at(1_000_000, &input, &state).unwrap();
    let (time, w) = checkpoint(&state_of(&response));
    assert_eq!(time, 1000);
    assert_eq!(w, weight(95));
}

#[test]
fn poke_at_end_records_end_weight() {
    let (input, state) = funded();
    // At end_timestamp or beyond, interpolate_weight clamps to end
    let response = poke_at(3_000_000, &input, &state).unwrap();
    let (time, w) = checkpoint(&state_of(&response));
    assert_eq!(time, 3000);
    assert_eq!(w, weight(50));
}

#[test]
fn repeated_poke_at_same_time_is_idempotent_no_event() {
    let (input, state) = funded();
    let first = poke_at(1_500_000, &input, &state).unwrap();
    assert_eq!(checkpoint_events(&first).len(), 1);
    let after_first = state_of(&first);
    let second = poke_at(1_500_000, &input, &after_first).unwrap();
    assert_eq!(checkpoint_events(&second).len(), 0);
    assert_eq!(checkpoint(&state_of(&second)), checkpoint(&after_first));
}

#[test]
fn poke_rejects_unfunded_pool() {
    let (_, prepared) = prepared();
    let state = lbp_core::PoolAccount::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let input = ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(id(99), id(8))),
        is_authorized: false,
        pre_state: borsh::to_vec(&state).unwrap().into(),
        message: borsh::to_vec(&Message::PokeWeighted).unwrap(),
    };
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::PendingFunding));
}

#[test]
fn buy_updates_checkpoint_on_new_timestamp() {
    let (input, state) = funded();
    assert_eq!(checkpoint(&state), (0, 0));
    let response = operation_at(
        1_500_000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    let after = state_of(&response);
    let (time, w) = checkpoint(&after);
    assert_eq!(time, 1500);
    assert!(w > 0);
    let poked = poke_at(1_500_000, &input, &state).unwrap();
    assert_checkpoint_events_match(&response, &poked);
    let repeated = poke_at(1_500_000, &input, &after).unwrap();
    assert!(checkpoint_events(&repeated).is_empty());
}

#[test]
fn buy_at_same_timestamp_does_not_change_checkpoint() {
    let (input, state) = funded();
    let first = operation_at(
        1_500_000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    let after_first = state_of(&first);
    let cp1 = checkpoint(&after_first);
    assert_eq!(cp1.0, 1500);
    let second = operation_at(
        1_500_000,
        &input,
        &after_first,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [110, after_first.remaining_inventory, 1000],
    )
    .unwrap();
    let cp2 = checkpoint(&state_of(&second));
    assert_eq!(cp1, cp2);
    assert!(checkpoint_events(&second).is_empty());
}

#[test]
fn buy_at_different_timestamp_updates_checkpoint() {
    let (input, state) = funded();
    let first = operation_at(
        1_500_000,
        &input,
        &state,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    )
    .unwrap();
    let after_first = state_of(&first);
    let cp1 = checkpoint(&after_first);
    let second = operation_at(
        2_000_000,
        &input,
        &after_first,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        config(0, id(43), false),
        [110, after_first.remaining_inventory, 1000],
    )
    .unwrap();
    let cp2 = checkpoint(&state_of(&second));
    assert_eq!(cp2.0, 2000);
    assert_ne!(cp1, cp2);
    let poked = poke_at(2_000_000, &input, &after_first).unwrap();
    assert_checkpoint_events_match(&second, &poked);
}

#[test]
fn exact_output_checkpoint_event_matches_poke_with_fractional_weights() {
    let (input, mut state) = funded();
    for (start, end) in [
        (weight(95) + 1, weight(50) + 7),
        (weight(50) + 7, weight(95) + 1),
    ] {
        if let lbp_core::PoolMode::Weighted {
            token_weight_start,
            token_weight_end,
            ..
        } = &mut state.mode
        {
            *token_weight_start = start;
            *token_weight_end = end;
        }
        let time_ms = 1_501_000;
        let response = operation_at(
            time_ms,
            &input,
            &state,
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
            config(0, id(43), false),
            [100, 1_000_000, 1000],
        )
        .unwrap();
        let poked = poke_at(time_ms, &input, &state).unwrap();
        assert_eq!(
            checkpoint(&state_of(&response)),
            checkpoint(&state_of(&poked))
        );
        assert_checkpoint_events_match(&response, &poked);
        let coll_w = interpolate_weight(
            lbp_core::WEIGHT_TOTAL - start,
            lbp_core::WEIGHT_TOTAL - end,
            1000,
            3000,
            1501,
        );
        assert_eq!(
            checkpoint(&state_of(&response)).1,
            lbp_core::WEIGHT_TOTAL - coll_w
        );
        let expected_in =
            calc_in_given_out(100, coll_w, 1_000_000, lbp_core::WEIGHT_TOTAL - coll_w, 500);
        assert_eq!(
            state_of(&response).recorded_collateral,
            state.recorded_collateral + expected_in
        );
    }
}

#[test]
fn failed_buy_does_not_change_checkpoint() {
    let (input, state) = funded();
    let poked = poke_at(1_500_000, &input, &state).unwrap();
    let after_poke = state_of(&poked);
    let cp = checkpoint(&after_poke);
    let result = operation_at(
        2_000_000,
        &input,
        &after_poke,
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: u128::MAX, // impossible slippage
        },
        config(0, id(43), false),
        [100, 1_000_000, 1000],
    );
    assert!(result.is_err());
    // Original state is unchanged (the pre_state would be restored by the VM)
    assert_eq!(checkpoint(&after_poke), cp);
}

#[test]
fn poke_on_settled_pool_still_works() {
    let (input, mut state) = funded();
    state.settlement_progress = lbp_core::SettlementProgress::ManualComplete;
    state.recorded_collateral = 0;
    let response = poke_at(1_500_000, &input, &state).unwrap();
    let (time, _) = checkpoint(&state_of(&response));
    assert_eq!(time, 1500);
}

#[test]
fn pending_poke_rechecks_saved_policy_and_schedule_before_clock_reply() {
    let (input, original) = funded();
    for case in 0..4 {
        let mut state = original.clone();
        match case {
            0 => state.ending_policy = lbp_core::EndingPolicy::Manual,
            1 => state.ending_policy = lbp_core::EndingPolicy::Depletion,
            2 => state.end_timestamp = None,
            _ => state.start_timestamp = u64::MAX,
        }
        let expected = if case < 2 {
            lbp_core::Error::InvalidState
        } else {
            lbp_core::Error::InvalidTerms
        };
        let callback = ReceiveInput {
            receiver: input.receiver,
            from: Some(lbp_core::pricing_clock::actor()),
            is_authorized: false,
            pre_state: borsh::to_vec(&(*b"LZLBPOKE", state)).unwrap().into(),
            message: borsh::to_vec(&clock_core::Reply::Timestamp(1_500_000)).unwrap(),
        };
        assert_eq!(handle(&callback).err(), Some(expected));
    }
}
