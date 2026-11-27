use super::fixture::{
    config, definition, funded, holding, id, namespace_state, operation, prepare, reply, state_of,
};
use lbp_core::{Message, handle};
use lee_core::account::{AccountId, Actor};

#[test]
fn fixed_price_preparation_keeps_the_bound_buy_policy() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
        unreachable!()
    };
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *inputs.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    inputs.buy_policy = policy.clone();
    input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
}

#[test]
fn fixed_price_allowlisted_buys_require_membership_before_external_queries() {
    let (mut input, mut state) = funded();
    state.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *state.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    for message in [
        Message::BuyExactInput {
            membership: None,
            amount_in: 30,
            min_amount_out: 0,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: None,
            amount_out: 10,
            max_amount_in: 30,
            now: 100,
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert!(matches!(handle(&input), Err(lbp_core::Error::Unauthorized)));
    }
}

fn membership_for(
    namespace: AccountId,
    pool: AccountId,
    trader: AccountId,
) -> (curve_core::BuyPolicy, curve_core::MembershipPath) {
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let path = curve_core::MembershipPath {
        index: 0,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let context = [&[1][..], namespace.value(), pool.value()].concat();
    let hash = |parts: &[&[u8]]| -> [u8; 32] {
        Impl::hash_bytes(&parts.concat())
            .as_bytes()
            .try_into()
            .unwrap()
    };
    let mut root = hash(&[
        b"LP/allowlist/leaf/v1\0",
        &context,
        trader.value(),
        &path.salt,
    ]);
    for sibling in &path.siblings {
        root = hash(&[b"LP/allowlist/node/v1\0", &context, &root, sibling]);
    }
    (
        curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: *namespace.value(),
            sale_policy_id: *pool.value(),
            root,
        }),
        path,
    )
}

#[test]
fn fixed_price_membership_can_be_reused_in_both_buy_forms_without_changing_economics() {
    let (input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    state.buy_policy = policy.clone();
    for message in [
        Message::BuyExactInput {
            membership: Some(path.clone()),
            amount_in: 30,
            min_amount_out: 19,
            now: 100,
        },
        Message::BuyExactOutput {
            membership: Some(path),
            amount_out: 10,
            max_amount_in: 16,
            now: 100,
        },
    ] {
        let response = operation(
            &input,
            &state,
            message,
            config(100, id(43), false),
            [
                state.recorded_collateral + state.accrued_fees,
                state.remaining_inventory,
                100,
            ],
        )
        .unwrap();
        state = state_of(&response);
        assert_eq!(state.buy_policy, policy);
        assert!(response.sync_messages.iter().any(|call| borsh::from_slice::<lez_compat_core::token::Message>(&call.message).is_ok_and(|message| matches!(message, lez_compat_core::token::Message::Transfer { recipient, .. } if recipient == id(50)))));
    }
    assert_eq!(
        (
            state.remaining_inventory,
            state.recorded_collateral,
            state.accrued_fees
        ),
        (771, 44, 2)
    );
}

#[test]
fn fixed_price_rejects_copied_and_malformed_membership_in_both_buy_forms() {
    let (mut input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    for case in 0..7 {
        state.buy_policy = policy.clone();
        let mut evidence = path.clone();
        input.from = Some(Actor::new(id(50), id(8)));
        match case {
            0 => input.from = Some(Actor::new(id(51), id(8))),
            1 => evidence.salt = [99; 32],
            2 => evidence.index = 1 << 17,
            3 => evidence.siblings[4] = [99; 32],
            4 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut state.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
            }
            5 => input.from = Some(Actor::new(id(50), id(99))),
            _ => input.from = Some(input.receiver),
        }
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for message in [
            Message::BuyExactInput {
                membership: Some(evidence.clone()),
                amount_in: 30,
                min_amount_out: 0,
                now: 100,
            },
            Message::BuyExactOutput {
                membership: Some(evidence.clone()),
                amount_out: 10,
                max_amount_in: 30,
                now: 100,
            },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(
                handle(&input).err(),
                Some(lbp_core::Error::Unauthorized),
                "case {case}"
            );
        }
    }
    input.from = Some(Actor::new(id(50), id(8)));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::BuyExactInput {
        membership: Some(path),
        amount_in: 30,
        min_amount_out: 0,
        now: 100,
    })
    .unwrap();
    input.message.pop();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}

#[test]
fn fixed_price_policy_survives_every_prepare_and_activation_callback() {
    let mut input = prepare();
    let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
    else {
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
    input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
    let factory = input.from;
    let mut response = handle(&input).unwrap();
    for activate in [false, true] {
        if activate {
            input.pre_state = response.post_state.clone().unwrap();
            input.from = factory;
            input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
            response = handle(&input).unwrap();
        }
        for bytes in [
            namespace_state(),
            definition(),
            definition(),
            holding(
                factory.unwrap().account_id,
                id(41),
                if activate { 0 } else { 800 },
            ),
            holding(
                input.receiver.account_id,
                id(41),
                if activate { 800 } else { 0 },
            ),
            holding(input.receiver.account_id, id(42), 0),
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
            assert_eq!(saved.ending_policy, lbp_core::EndingPolicy::Either);
        }
        assert_eq!(
            lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap())
                .unwrap()
                .funded,
            activate
        );
    }
}
