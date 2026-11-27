use super::fixture::{config, funded, id, operation, state_of};
use lbp_core::{Message, handle};
use lee_core::account::{AccountId, Actor};

#[test]
fn weighted_allowlisted_buys_require_membership_before_external_queries() {
    let (mut input, mut state) = funded();
    state.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *state.namespace.value(),
        sale_policy_id: *input.receiver.account_id.value(),
        root: [9; 32],
    });
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    for message in [
        Message::BuyWeightedExactInput {
            membership: None,
            amount_in: 10,
            min_amount_out: 1,
        },
        Message::BuyWeightedExactOutput {
            membership: None,
            amount_out: 500,
            max_amount_in: 1000,
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert_eq!(handle(&input).err(), Some(lbp_core::Error::Unauthorized));
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
fn weighted_membership_can_be_reused_in_both_buy_forms_without_changing_economics() {
    let (input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    state.buy_policy = policy.clone();
    for (approved, open) in [
        (
            Message::BuyWeightedExactInput {
                membership: Some(path.clone()),
                amount_in: 10,
                min_amount_out: 1,
            },
            Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 10,
                min_amount_out: 1,
            },
        ),
        (
            Message::BuyWeightedExactOutput {
                membership: Some(path),
                amount_out: 500,
                max_amount_in: 1000,
            },
            Message::BuyWeightedExactOutput {
                membership: None,
                amount_out: 500,
                max_amount_in: 1000,
            },
        ),
    ] {
        let balances = [
            state.recorded_collateral + state.accrued_fees,
            state.remaining_inventory,
            1000,
        ];
        let response = operation(
            &input,
            &state,
            approved,
            config(100, id(43), false),
            balances,
        )
        .unwrap();
        let mut public = state.clone();
        public.buy_policy = curve_core::BuyPolicy::Open;
        let expected =
            operation(&input, &public, open, config(100, id(43), false), balances).unwrap();
        state = state_of(&response);
        assert_eq!(state.buy_policy, policy);
        let mut actual = state.clone();
        actual.buy_policy = curve_core::BuyPolicy::Open;
        assert_eq!(actual, state_of(&expected));
        assert_eq!(response.sync_messages, expected.sync_messages);
    }
    assert!(state.remaining_inventory < 999_500);
    assert!(state.recorded_collateral > 100);
}

#[test]
fn weighted_rejects_copied_and_malformed_membership_in_both_buy_forms() {
    let (mut input, mut state) = funded();
    let (policy, path) = membership_for(state.namespace, input.receiver.account_id, id(50));
    for case in 0..10 {
        state.buy_policy = policy.clone();
        let mut evidence = path.clone();
        input.from = Some(Actor::new(id(50), id(8)));
        let expected = match case {
            0 => {
                input.from = Some(Actor::new(id(51), id(8)));
                lbp_core::Error::Unauthorized
            }
            1 => {
                evidence.salt = [99; 32];
                lbp_core::Error::Unauthorized
            }
            2 => {
                evidence.index = 1 << 17;
                lbp_core::Error::Unauthorized
            }
            3 => {
                evidence.siblings[4] = [99; 32];
                lbp_core::Error::Unauthorized
            }
            4 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut state.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
                lbp_core::Error::Unauthorized
            }
            5 => {
                input.from = Some(Actor::new(id(50), id(99)));
                lbp_core::Error::Unauthorized
            }
            6 => {
                input.from = Some(input.receiver);
                lbp_core::Error::Unauthorized
            }
            7..=9 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut state.buy_policy else {
                    unreachable!()
                };
                match case {
                    7 => policy.version = 2,
                    8 => policy.namespace = [99; 32],
                    _ => policy.sale_policy_id = [99; 32],
                }
                lbp_core::Error::InvalidTerms
            }
            _ => unreachable!(),
        };
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        for message in [
            Message::BuyWeightedExactInput {
                membership: Some(evidence.clone()),
                amount_in: 10,
                min_amount_out: 1,
            },
            Message::BuyWeightedExactOutput {
                membership: Some(evidence.clone()),
                amount_out: 500,
                max_amount_in: 1000,
            },
        ] {
            input.message = borsh::to_vec(&message).unwrap();
            assert_eq!(handle(&input).err(), Some(expected), "case {case}");
        }
    }
    state.buy_policy = policy;
    input.from = Some(Actor::new(id(50), id(8)));
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::BuyWeightedExactInput {
        membership: Some(path),
        amount_in: 10,
        min_amount_out: 1,
    })
    .unwrap();
    input.message.pop();
    assert_eq!(handle(&input).err(), Some(lbp_core::Error::InvalidMessage));
}
