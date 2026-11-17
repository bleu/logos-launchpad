use lbp_core::{CreatorAllocationRelease, Message, WeightedPoolInputs, compute_pool_pda, handle};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput},
};

fn weight(percent: u128) -> u128 {
    percent * lbp_core::WEIGHT_TOTAL / 100
}

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

fn weighted_inputs() -> WeightedPoolInputs {
    WeightedPoolInputs {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: lbp_core::PausePolicy::AuthorityControlled,
        ending_policy: lbp_core::EndingPolicy::Scheduled,
        trade_mode: lbp_core::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_adapter: None,
        graduation_policy: sale_policy::GraduationPolicy::default(),
        namespace: id(40),
        namespace_program_id: id(6),
        factory_program_id: id(7),
        factory_seed: [3; 32],
        token_definition_id: id(41),
        collateral_definition_id: id(42),
        sale_inventory: 1_000_000,
        collateral_seed: 100,
        creator_allocation: 0,
        token_weight_start: weight(95),
        token_weight_end: weight(50),
        start_timestamp: 100,
        end_timestamp: 200,
        creator_allocation_release: CreatorAllocationRelease::AfterSetup,
    }
}

fn prepare() -> ReceiveInput {
    let inputs = weighted_inputs();
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(factory, id(7))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap(),
    }
}

fn namespace_state() -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace: id(40),
            protocol_fee_bps: 100,
            treasury: id(43),
        })
        .unwrap(),
    };
    curve_core::handle(&input)
        .unwrap()
        .post_state
        .unwrap()
        .to_vec()
}

fn reply(
    input: &ReceiveInput,
    response: &lee_core::program::Response,
    from: Actor,
    bytes: Vec<u8>,
) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(from),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&lee_core::program::StateReply {
            state: bytes.into(),
        })
        .unwrap(),
    }
}

fn definition() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Token".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 2_000_000,
        metadata: None,
    })
    .unwrap()
}

fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Holding {
        owner,
        definition,
        kind: lez_compat_core::token::TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

/// Drive the prepare → namespace → definitions → custody handshake through
/// to completion. Returns (final_input, final_response).
fn prepared() -> (ReceiveInput, lee_core::program::Response) {
    let inputs = weighted_inputs();
    let factory = prepare().from.unwrap().account_id;
    let mut input = prepare();
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(factory, id(41), inputs.sale_inventory),
        holding(input.receiver.account_id, id(41), 0),
        holding(input.receiver.account_id, id(42), 0),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}

// ---------------------------------------------------------------------------
// PrepareWeighted — term validation
// ---------------------------------------------------------------------------

#[test]
fn weighted_preparation_rejects_invalid_terms() {
    for case in 0..10u8 {
        let mut input = prepare();
        let Message::PrepareWeighted { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                input.pre_state = vec![1].into();
                lbp_core::Error::AlreadyInitialized
            }
            1 => {
                input.receiver.account_id = id(99);
                lbp_core::Error::WrongAccount
            }
            2 => {
                input.from = None;
                lbp_core::Error::Unauthorized
            }
            3 => {
                inputs.sale_inventory = 0;
                lbp_core::Error::InvalidTerms
            }
            4 => {
                inputs.token_weight_start = 0;
                lbp_core::Error::InvalidTerms
            }
            5 => {
                inputs.token_weight_end = 0;
                lbp_core::Error::InvalidTerms
            }
            6 => {
                // A token weight equal to the total gives a zero collateral weight.
                inputs.token_weight_start = lbp_core::WEIGHT_TOTAL;
                lbp_core::Error::InvalidTerms
            }
            7 => {
                // end_timestamp <= start_timestamp
                inputs.end_timestamp = 100;
                lbp_core::Error::InvalidTerms
            }
            8 => {
                inputs.token_definition_id = inputs.collateral_definition_id;
                lbp_core::Error::InvalidTerms
            }
            _ => {
                inputs.collateral_seed = 0;
                lbp_core::Error::InvalidTerms
            }
        };
        if case != 2 {
            input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
        }
        assert!(
            matches!(handle(&input), Err(e) if e == expected),
            "case {case}: expected {expected:?}"
        );
    }
}

#[test]
fn weighted_preparation_checks_namespace_through_its_actor() {
    let input = prepare();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6))
    );
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&response.sync_messages[0].message).unwrap(),
        curve_core::Message::ReadNamespace { namespace: id(40) }
    );
}

#[test]
fn weighted_preparation_saves_pending_terms_and_initializes_vaults() {
    let (input, response) = prepared();
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(!state.funded);
    assert_eq!(state.recorded_collateral, 0);
    assert_eq!(state.accrued_fees, 0);
    assert_eq!(
        (state.start_timestamp, state.end_timestamp),
        (100, Some(200))
    );
    assert_eq!(state.sale_inventory, 1_000_000);
    assert_eq!(state.remaining_inventory, 1_000_000);
    assert_eq!(state.factory_owner, prepare().from.unwrap().account_id);
    assert_eq!(
        input.receiver.account_id,
        compute_pool_pda(id(40), id(8), id(41), id(42), state.factory_owner)
    );
    assert!(response.async_messages.is_empty());
    // mode must be Weighted
    assert!(matches!(state.mode, lbp_core::PoolMode::Weighted { .. }));
}

// ---------------------------------------------------------------------------
// ActivateWeighted
// ---------------------------------------------------------------------------

fn activation(
    token_vault: u128,
    collateral_vault: u128,
) -> (ReceiveInput, lee_core::program::Response) {
    let (mut input, prepared) = prepared();
    input.pre_state = prepared.post_state.unwrap();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    let mut response = handle(&input).unwrap();
    let inputs = weighted_inputs();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(input.from.unwrap().account_id, id(41), 0),
        holding(input.receiver.account_id, id(41), token_vault),
        holding(input.receiver.account_id, id(42), collateral_vault),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    let _ = inputs;
    (input, response)
}

#[test]
fn weighted_activation_funds_and_records_seed_collateral() {
    let inputs = weighted_inputs();
    let (mut input, response) = activation(inputs.sale_inventory, inputs.collateral_seed);
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(state.funded);
    assert_eq!(state.remaining_inventory, inputs.sale_inventory);
    // recorded_collateral starts at the seed so the vault liabilities balance
    assert_eq!(state.recorded_collateral, inputs.collateral_seed);
    assert_eq!(state.accrued_fees, 0);
    assert!(matches!(state.mode, lbp_core::PoolMode::Weighted {
        collateral_seed,
        pricing_collateral,
        ..
    } if collateral_seed == inputs.collateral_seed && pricing_collateral == inputs.collateral_seed));
    // double-funding must fail
    input.pre_state = response.post_state.unwrap();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    assert!(matches!(
        handle(&input),
        Err(lbp_core::Error::AlreadyFunded)
    ));
}

#[test]
fn weighted_activation_excludes_collateral_surplus_from_pricing() {
    let inputs = weighted_inputs();
    let (_, response) = activation(inputs.sale_inventory, inputs.collateral_seed + 999);
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.recorded_collateral, inputs.collateral_seed);
    assert!(matches!(state.mode, lbp_core::PoolMode::Weighted {
        pricing_collateral,
        ..
    } if pricing_collateral == inputs.collateral_seed));
}

#[test]
fn weighted_activation_rejects_wrong_token_vault_balance() {
    let inputs = weighted_inputs();
    for wrong in [0u128, inputs.sale_inventory - 1, inputs.sale_inventory + 1] {
        let (mut input, prepared) = prepared();
        input.pre_state = prepared.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
        let mut response = handle(&input).unwrap();
        for (index, bytes) in [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), 0),
            holding(input.receiver.account_id, id(41), wrong),
        ]
        .into_iter()
        .enumerate()
        {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            let result = handle(&input);
            if index == 4 {
                assert!(
                    matches!(result, Err(lbp_core::Error::InvalidCustody)),
                    "wrong={wrong}"
                );
                break;
            }
            response = result.unwrap();
        }
    }
}

#[test]
fn weighted_preparation_saves_the_initial_bound_buy_policy() {
    let mut input = prepare();
    let mut inputs = weighted_inputs();
    inputs.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *inputs.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    input.message = borsh::to_vec(&Message::PrepareWeighted {
        inputs: inputs.clone(),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        inputs.buy_policy
    );
}

#[test]
fn weighted_preparation_rejects_wrong_policy_version_namespace_or_pool() {
    for case in 0..3 {
        let mut input = prepare();
        let mut inputs = weighted_inputs();
        inputs.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: if case == 0 { 2 } else { 1 },
            namespace: if case == 1 {
                [99; 32]
            } else {
                *inputs.namespace.value()
            },
            sale_policy_id: if case == 2 {
                [99; 32]
            } else {
                *input.receiver.account_id.value()
            },
            root: [9; 32],
        });
        input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
        assert_eq!(
            handle(&input).err(),
            Some(lbp_core::Error::InvalidTerms),
            "case {case}"
        );
    }
}

#[test]
fn weighted_policy_survives_every_prepare_and_activation_callback() {
    let mut input = prepare();
    let Message::PrepareWeighted { mut inputs } = borsh::from_slice(&input.message).unwrap() else {
        unreachable!()
    };
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *inputs.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    inputs.buy_policy = policy.clone();
    inputs.trade_mode = lbp_core::TradeMode::BuyAndSell;
    inputs.sell_fee_bps = 250;
    inputs.graduation_adapter = Some(Actor::new(id(70), id(71)));
    input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
    let factory = input.from;
    let mut response = handle(&input).unwrap();
    for activate in [false, true] {
        if activate {
            input.pre_state = response.post_state.clone().unwrap();
            input.from = factory;
            input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
            response = handle(&input).unwrap();
        }
        for bytes in [
            namespace_state(),
            definition(),
            definition(),
            holding(
                factory.unwrap().account_id,
                id(41),
                if activate { 0 } else { 1_000_000 },
            ),
            holding(
                input.receiver.account_id,
                id(41),
                if activate { 1_000_000 } else { 0 },
            ),
            holding(
                input.receiver.account_id,
                id(42),
                if activate { 100 } else { 0 },
            ),
        ] {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            response = handle(&input).unwrap();
            let saved =
                lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
            assert_eq!(saved.buy_policy, policy);
            assert_eq!(saved.trade_mode, lbp_core::TradeMode::BuyAndSell);
            assert_eq!(saved.sell_fee_bps, 250);
            assert_eq!(saved.graduation_adapter, Some(Actor::new(id(70), id(71))));
            assert_eq!(
                saved.settlement_progress,
                lbp_core::SettlementProgress::Unselected
            );
            assert_eq!(saved.ending_policy, lbp_core::EndingPolicy::Scheduled);
        }
        assert_eq!(
            lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap())
                .unwrap()
                .funded,
            activate
        );
    }
}

#[test]
fn weighted_preparation_preserves_each_explicit_pause_policy() {
    for policy in [
        lbp_core::PausePolicy::Disabled,
        lbp_core::PausePolicy::AuthorityControlled,
    ] {
        let mut input = prepare();
        let mut inputs = weighted_inputs();
        inputs.pause_policy = policy;
        input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
        let response = handle(&input).unwrap();
        let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.trade_control.policy(), policy);
        assert!(!state.trade_control.is_paused());
    }
}

#[test]
fn weighted_policy_matrix_rejects_unsupported_terms_before_setup_queries() {
    use lbp_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        let mut input = prepare();
        let mut inputs = weighted_inputs();
        inputs.ending_policy = ending;
        input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
        let result = handle(&input);
        if matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either) {
            let response = result.unwrap();
            let state =
                lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
            assert_eq!(state.ending_policy, ending);
        } else {
            assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
        }
    }
}

#[test]
fn weighted_preparation_checks_both_timestamp_conversions_before_queries() {
    let last = u64::MAX / 1000;
    for (start, end, valid) in [
        (0, 1, true),
        (last - 1, last, true),
        (last, last + 1, false),
        (last + 1, last + 2, false),
        (0, u64::MAX, false),
        (100, 100, false),
    ] {
        let mut input = prepare();
        let mut inputs = weighted_inputs();
        inputs.start_timestamp = start;
        inputs.end_timestamp = end;
        input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
        let result = handle(&input);
        if valid {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
        }
    }
}

#[test]
fn malformed_weighted_policies_cannot_bypass_pool_entry_or_setup_replies() {
    let setup = prepare();
    let response = handle(&setup).unwrap();
    let base = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    let (_, prepared_response) = prepared();
    let ready =
        lbp_core::PoolAccount::decode(prepared_response.post_state.as_ref().unwrap()).unwrap();
    for case in 0..6 {
        let mut state = base.clone();
        match case {
            0 => state.ending_policy = lbp_core::EndingPolicy::Manual,
            1 => state.ending_policy = lbp_core::EndingPolicy::Depletion,
            2 => state.ending_policy = lbp_core::EndingPolicy::Manual,
            3 => state.end_timestamp = None,
            4 => state.start_timestamp = u64::MAX,
            _ => state.end_timestamp = Some(u64::MAX),
        }
        let expected = if case < 3 {
            lbp_core::Error::InvalidState
        } else {
            lbp_core::Error::InvalidTerms
        };
        let mut callback = reply(
            &setup,
            &response,
            response.sync_messages[0].to,
            namespace_state(),
        );
        callback.pre_state = borsh::to_vec(&state).unwrap().into();
        assert_eq!(handle(&callback).err(), Some(expected));
        let mut active = ready.clone();
        active.funded = true;
        active.ending_policy = state.ending_policy;
        active.start_timestamp = state.start_timestamp;
        active.end_timestamp = state.end_timestamp;
        for message in [
            Message::ReadState,
            Message::ActivateWeighted,
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 10,
                min_amount_out: 0,
            },
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 100,
                max_amount_in: 100,
            },
            Message::SetPaused { paused: false },
            Message::SetWeightedPaused { paused: false },
            Message::SettleWeighted { creator: id(60) },
            Message::CollectFees,
            Message::PokeWeighted,
        ] {
            let mut input = setup.clone();
            input.pre_state = borsh::to_vec(&active).unwrap().into();
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(expected));
        }
    }
}

#[test]
fn snapshot_quotes_work_without_a_provider_but_weighted_sells_require_one() {
    let (mut input, response) = activation(1_000_000, 100);
    let mut state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.trade_mode = lbp_core::TradeMode::BuyAndSell;
    state.recorded_collateral = 1_000_000;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral,
        token_weight_start,
        token_weight_end,
        ..
    } = &mut state.mode
    {
        *pricing_collateral = 1_000_000;
        *token_weight_start = weight(50);
        *token_weight_end = weight(50);
    }
    let quoted = lbp_core::quote_trade(
        &state,
        lbp_core::QuoteRequest::SellExactInput {
            tokens_in: 1000,
            min_collateral_out: 0,
        },
        100,
        150,
    )
    .unwrap();
    assert!(quoted.amount_out > 0);
    if !cfg!(local_mock_clock) {
        input.from = Some(Actor::new(id(50), input.receiver.program_account_id));
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.message = borsh::to_vec(&Message::SellWeightedExactInput {
            amount_in: 1000,
            min_amount_out: 0,
        })
        .unwrap();
        assert_eq!(
            handle(&input).err(),
            Some(lbp_core::Error::ClockUnavailable)
        );
    }
}

#[test]
fn weighted_rejects_invalid_stored_sell_fee() {
    let mut input = prepare();
    let Message::PrepareWeighted { mut inputs } = borsh::from_slice(&input.message).unwrap() else {
        unreachable!()
    };
    for invalid in [10_000, u16::MAX] {
        inputs.sell_fee_bps = invalid;
        input.message = borsh::to_vec(&Message::PrepareWeighted {
            inputs: inputs.clone(),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidTerms));
    }
}

#[test]
fn weighted_pool_preparation_validates_saved_graduation_percentages() {
    for has_adapter in [false, true] {
        for policy in [
            lbp_core::GraduationPolicy::default(),
            lbp_core::GraduationPolicy {
                launch_bps: 10_000,
                collateral_bps: 0,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_000,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 10_001,
                collateral_bps: 0,
            },
            lbp_core::GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 10_001,
            },
        ] {
            let mut input = prepare();
            let Message::PrepareWeighted { mut inputs } =
                borsh::from_slice(&input.message).unwrap()
            else {
                unreachable!()
            };
            inputs.graduation_adapter = has_adapter.then(|| Actor::new(id(70), id(71)));
            inputs.graduation_policy = policy;
            input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
            let result = handle(&input);
            if policy.validate(has_adapter).is_err() {
                assert_eq!(result.err(), Some(lbp_core::Error::InvalidTerms));
            } else {
                assert_eq!(
                    lbp_core::PoolAccount::decode(result.unwrap().post_state.as_ref().unwrap())
                        .unwrap()
                        .graduation_policy,
                    policy
                );
            }
        }
    }
}
