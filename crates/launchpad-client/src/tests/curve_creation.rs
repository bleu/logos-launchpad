use factory_core::CreationStage;
use lee::AccountId;
use lee_core::account::Actor;

use crate::curve::{factory_pool_addresses, validate_factory_state};
use crate::invocation::associated_token_account;
use crate::{
    AllowlistPolicy, BuyPolicy, ClientError, ExactInputTradeTerms, ExactOutputTradeTerms,
    GraduationPolicy, build_buy_invocation, build_buy_with_collateral_invocation,
    build_create_sale_invocation, build_sell_invocation, curve_buy_policy,
    next_creation_invocation,
};

use super::fixtures::{CURVE_PROGRAM_ID, FACTORY_PROGRAM_ID, creation, sale, valid_factory_state};

#[test]
fn creation_preserves_the_allowlist_policy_on_the_factory_message() {
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *sale().namespace.value(),
        sale_policy_id: *factory_pool_addresses(sale()).pool.value(),
        root: [42; 32],
    });
    let mut request = creation();
    request.buy_policy = curve_buy_policy(sale(), Some([42; 32]));
    let call = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).unwrap();
    let factory_core::Message::CreateFactoryPool { buy_policy, .. } = call.instruction else {
        panic!("initial creation message");
    };
    assert_eq!(buy_policy, policy);
}

#[test]
fn curve_creation_rejects_allowlist_policies_bound_to_another_sale() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let policy = AllowlistPolicy {
        version: 1,
        namespace: *sale.namespace.value(),
        sale_policy_id: *factory_pool_addresses(sale).pool.value(),
        root: [42; 32],
    };
    for invalid in [
        AllowlistPolicy {
            version: 2,
            ..policy.clone()
        },
        AllowlistPolicy {
            namespace: [2; 32],
            ..policy.clone()
        },
        AllowlistPolicy {
            sale_policy_id: [3; 32],
            ..policy
        },
    ] {
        let mut request = creation();
        request.buy_policy = BuyPolicy::Allowlist(invalid);
        assert!(matches!(
            build_create_sale_invocation(sale, creator, request),
            Err(ClientError::FactoryPolicy(
                factory_core::Error::InvalidTerms
            ))
        ));
    }
}

#[test]
fn creation_request_encodes_the_required_typed_mode() {
    for (mode, wire) in [
        (factory_core::TradeMode::BuyOnly, 0),
        (factory_core::TradeMode::BuyAndSell, 1),
    ] {
        let mut request = creation();
        request.trade_mode = mode;
        let call = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).unwrap();
        assert!(
            matches!(call.instruction, factory_core::Instruction::CreateFactoryPool { trade_mode, .. } if trade_mode == wire)
        );
    }
}

#[test]
fn atomic_create_skips_a_matching_sale_and_rejects_partial_setup() {
    let creator = AccountId::new([9; 32]);
    let mut state = valid_factory_state(sale());
    assert!(
        next_creation_invocation(sale(), creator, creation(), Some(&state))
            .unwrap()
            .is_none()
    );
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
    ] {
        state.creation_stage = stage;
        assert!(matches!(
            next_creation_invocation(sale(), creator, creation(), Some(&state)),
            Err(ClientError::CreationIncomplete)
        ));
    }
}

#[test]
fn matching_active_sale_requires_the_same_trade_mode() {
    let state = valid_factory_state(sale());
    let mut request = creation();
    assert!(
        next_creation_invocation(
            sale(),
            AccountId::new([9; 32]),
            request.clone(),
            Some(&state)
        )
        .is_ok()
    );
    request.trade_mode = factory_core::TradeMode::BuyOnly;
    assert!(matches!(
        next_creation_invocation(sale(), AccountId::new([9; 32]), request, Some(&state)),
        Err(ClientError::LaunchParametersMismatch)
    ));
}

#[test]
fn matching_active_sale_requires_the_same_allowlist() {
    let policy = curve_core::AllowlistPolicy {
        version: 1,
        namespace: *sale().namespace.value(),
        sale_policy_id: *factory_pool_addresses(sale()).pool.value(),
        root: [42; 32],
    };
    let mut state = valid_factory_state(sale());
    state.buy_policy = curve_core::BuyPolicy::Allowlist(policy.clone());
    let mut request = creation();
    request.buy_policy = state.buy_policy.clone();
    assert!(
        next_creation_invocation(
            sale(),
            AccountId::new([9; 32]),
            request.clone(),
            Some(&state)
        )
        .is_ok()
    );
    let mut changed_root = policy.clone();
    changed_root.root = [43; 32];
    for replacement in [
        curve_core::BuyPolicy::Open,
        curve_core::BuyPolicy::Allowlist(changed_root),
    ] {
        request.buy_policy = replacement;
        assert!(matches!(
            next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            ),
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
}

#[test]
fn matching_active_sale_requires_the_same_token_metadata() {
    let state = valid_factory_state(sale());
    let mut request = creation();
    request.name = "Changed token".into();
    assert!(matches!(
        next_creation_invocation(sale(), AccountId::new([9; 32]), request, Some(&state)),
        Err(ClientError::LaunchParametersMismatch)
    ));
}

#[test]
fn factory_launch_builds_derived_accounts_and_creator_authorization() {
    let creator = AccountId::new([9; 32]);
    let call = build_create_sale_invocation(sale(), creator, creation()).unwrap();
    assert_eq!(
        call.destination,
        Actor::new(factory_pool_addresses(sale()).factory, FACTORY_PROGRAM_ID)
    );
    assert_eq!(call.signer_accounts, [creator]);
    let role = factory_core::compute_curve_creator_role_pda(
        sale().namespace,
        FACTORY_PROGRAM_ID,
        sale().launch_salt,
    );
    assert!(
        call.actors
            .contains(&Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID))
    );
    assert!(call.actors.contains(&Actor::new(
        creator,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
    )));
    assert!(call.account_mentions().iter().any(|mention| mention.actor()
        == Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        && !mention.authorizes
        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. })));
}

#[test]
fn factory_creation_admits_derived_launch_accounts_and_canonical_holdings() {
    let creator = AccountId::new([9; 32]);
    let call = build_create_sale_invocation(sale(), creator, creation()).unwrap();
    let a = factory_pool_addresses(sale());
    let mentions = call.account_mentions();
    for account in [
        a.factory,
        a.token_definition,
        a.pool,
        factory_core::compute_mint_pda(
            sale().namespace,
            sale().factory_program_id,
            sale().launch_salt,
        ),
        associated_token_account(
            factory_core::compute_mint_pda(
                sale().namespace,
                sale().factory_program_id,
                sale().launch_salt,
            ),
            a.token_definition,
        ),
        associated_token_account(a.creator_escrow, a.token_definition),
        factory_core::compute_curve_creator_role_pda(
            sale().namespace,
            FACTORY_PROGRAM_ID,
            sale().launch_salt,
        ),
    ] {
        let mention = mentions
            .iter()
            .find(|m| m.actor().account_id == account)
            .unwrap();
        assert!(!mention.authorizes);
        assert!(
            matches!(&mention.identity, wallet::AccountIdentity::PublicPda { .. }),
            "{account}"
        );
    }
}

#[test]
fn factory_launch_rejects_a_virtual_token_reserve_at_the_sale_target() {
    let mut request = creation();
    request.virtual_token_reserve = request.sale_reserve;
    let error = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).unwrap_err();

    assert!(matches!(
        error,
        ClientError::FactoryParameters(
            factory_core::CreateError::VirtualTokenReserveNotAboveSaleReserve
        )
    ));
}

#[test]
fn namespace_selection_changes_factory_pool_and_swap_config_together() {
    let mut other = sale();
    other.namespace = AccountId::new([0xAE; 32]);
    let terms = ExactOutputTradeTerms {
        amount_out: 10,
        max_amount_in: 100,
    };
    let first = build_buy_invocation(sale(), AccountId::new([3; 32]), terms, None, 0);
    let second = build_buy_invocation(other, AccountId::new([3; 32]), terms, None, 0);
    assert_ne!(first.instruction, second.instruction);
    assert!(first.actors.contains(&Actor::new(
        curve_core::compute_config_pda(sale().namespace, CURVE_PROGRAM_ID),
        CURVE_PROGRAM_ID
    )));
    assert!(second.actors.contains(&Actor::new(
        curve_core::compute_config_pda(other.namespace, CURVE_PROGRAM_ID),
        CURVE_PROGRAM_ID
    )));
}

#[test]
fn creation_and_trade_builders_use_one_factory_sale_context() {
    let participant = AccountId::new([9; 32]);
    let exact_output = build_buy_invocation(
        sale(),
        participant,
        ExactOutputTradeTerms {
            amount_out: 25,
            max_amount_in: 100,
        },
        None,
        0,
    );
    let exact_input = build_buy_with_collateral_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        None,
        0,
    );
    let sell = build_sell_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        0,
    );
    assert_eq!(
        exact_output.destination,
        Actor::new(participant, CURVE_PROGRAM_ID)
    );
    assert_eq!(exact_output.destination, exact_input.destination);
    assert_eq!(sell.destination, exact_input.destination);
    assert_eq!(exact_output.signer_accounts, [participant]);
}

#[test]
fn factory_instruction_wire_slots_follow_the_source_idl_order() {
    for (message, expected) in [
        (
            factory_core::Message::ContinueCreation {
                creator: AccountId::new([1; 32]),
                now: 0,
            },
            1,
        ),
        (
            factory_core::Message::ClaimCreatorAllocation {
                creator: AccountId::new([1; 32]),
                now: 0,
            },
            2,
        ),
        (
            factory_core::Message::WithdrawFactoryProceeds {
                creator: AccountId::new([1; 32]),
                now: 0,
            },
            3,
        ),
    ] {
        assert_eq!(borsh::to_vec(&message).unwrap()[0], expected);
    }
}

#[test]
fn creation_admits_the_factory_curve_relay_without_authorization() {
    let selected = sale();
    let factory = factory_core::compute_factory_pda(
        selected.namespace,
        selected.factory_program_id,
        selected.launch_salt,
    );
    let call = build_create_sale_invocation(selected, AccountId::new([9; 32]), creation()).unwrap();
    let mentions = call.account_mentions();
    let relay = mentions
        .iter()
        .find(|mention| mention.actor() == Actor::new(factory, selected.curve_program_id))
        .expect("factory curve relay");
    assert!(!relay.authorizes);
    assert!(matches!(relay.identity,
        wallet::AccountIdentity::PublicPda { program, seed }
            if program == selected.factory_program_id
                && seed == factory_core::compute_factory_seed(selected.namespace, selected.launch_salt)
    ));
    assert_eq!(call.message_bytes().unwrap()[0], 0);
}

#[test]
fn creation_preserves_pause_policy_and_checks_it_on_the_active_sale() {
    for policy in [
        curve_core::PausePolicy::Disabled,
        curve_core::PausePolicy::AuthorityControlled,
    ] {
        let mut request = creation();
        request.pause_policy = policy;
        let call =
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()).unwrap();
        assert!(
            matches!(call.instruction, factory_core::Message::CreateFactoryPool { pause_policy, .. } if pause_policy == policy)
        );
        let mut state = valid_factory_state(sale());
        state.pause_policy = policy;
        assert!(
            next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        state.pause_policy = match policy {
            curve_core::PausePolicy::Disabled => curve_core::PausePolicy::AuthorityControlled,
            curve_core::PausePolicy::AuthorityControlled => curve_core::PausePolicy::Disabled,
        };
        assert!(matches!(
            next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            ),
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
}

#[test]
fn creation_accepts_only_factory_ending_windows() {
    use curve_core::EndingPolicy::{Depletion, Either, Manual, Scheduled};
    for ending in [Manual, Scheduled, Depletion, Either] {
        for end in [None, Some(400)] {
            let mut request = creation();
            request.ending_policy = ending;
            request.end_timestamp = end;
            let result = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request);
            let supported = matches!(
                (ending, end),
                (Depletion, None) | (Scheduled | Either, Some(_))
            );
            if supported {
                assert!(matches!(
                    result.unwrap().instruction,
                    factory_core::Message::CreateFactoryPool { ending_policy, end_timestamp, .. }
                        if ending_policy == ending && end_timestamp == end
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(ClientError::FactoryPolicy(
                        factory_core::Error::InvalidTerms
                    ))
                ));
            }
        }
    }
}

#[test]
fn matching_active_sale_requires_the_same_ending_policy_and_deadline() {
    use curve_core::EndingPolicy::{Depletion, Either, Scheduled};
    for (ending, end) in [
        (Depletion, None),
        (Scheduled, Some(400)),
        (Either, Some(400)),
    ] {
        let mut state = valid_factory_state(sale());
        state.ending_policy = ending;
        state.end_timestamp = end;
        let mut request = creation();
        request.ending_policy = ending;
        request.end_timestamp = end;
        assert!(
            next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        for (replacement, deadline) in [
            (Depletion, None),
            (Scheduled, Some(400)),
            (Either, Some(400)),
            (ending, Some(401)),
        ] {
            if (replacement, deadline) == (ending, end) {
                continue;
            }
            let mut changed = request.clone();
            changed.ending_policy = replacement;
            changed.end_timestamp = deadline;
            assert!(matches!(
                next_creation_invocation(sale(), AccountId::new([9; 32]), changed, Some(&state)),
                Err(ClientError::LaunchParametersMismatch)
            ));
        }
    }
}

#[test]
fn matching_active_sale_requires_the_same_graduation_hook() {
    let hook = factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![3, 7],
    };
    let mut state = valid_factory_state(sale());
    state.graduation_hook = Some(hook.clone());
    let mut request = creation();
    request.graduation_hook = Some(hook.clone());
    assert!(
        next_creation_invocation(
            sale(),
            AccountId::new([9; 32]),
            request.clone(),
            Some(&state)
        )
        .is_ok()
    );
    for changed in [
        None,
        Some(factory_core::GraduationHook {
            program_id: AccountId::new([20; 32]),
            ..hook.clone()
        }),
        Some(factory_core::GraduationHook {
            configuration: vec![3, 8],
            ..hook.clone()
        }),
    ] {
        request.graduation_hook = changed;
        assert!(matches!(
            next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            ),
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
}

#[test]
fn creation_forwards_sell_fee_and_checks_it_on_the_active_sale() {
    let mut request = creation();
    request.sell_fee_bps = 250;
    let call =
        build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()).unwrap();
    assert!(matches!(
        call.instruction,
        factory_core::Message::CreateFactoryPool {
            sell_fee_bps: 250,
            ..
        }
    ));
    let mut state = valid_factory_state(sale());
    state.sell_fee_bps = 250;
    assert!(
        next_creation_invocation(
            sale(),
            AccountId::new([9; 32]),
            request.clone(),
            Some(&state)
        )
        .is_ok()
    );
    for replacement in [0, 251] {
        let mut changed = request.clone();
        changed.sell_fee_bps = replacement;
        assert!(matches!(
            next_creation_invocation(sale(), AccountId::new([9; 32]), changed, Some(&state)),
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
    for rate in [10_000, u16::MAX] {
        request.sell_fee_bps = rate;
        assert!(matches!(
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()),
            Err(ClientError::FactoryPolicy(
                factory_core::Error::InvalidTerms
            ))
        ));
        let mut state = valid_factory_state(sale());
        state.sell_fee_bps = rate;
        assert!(matches!(
            validate_factory_state(sale(), &state),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
}

#[test]
fn curve_creation_preserves_and_validates_graduation_percentages() {
    for policy in [
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
        GraduationPolicy {
            launch_bps: 10000,
            collateral_bps: 10000,
        },
    ] {
        let mut request = creation();
        request.graduation_hook = Some(factory_core::GraduationHook {
            program_id: AccountId::new([19; 32]),
            configuration: vec![],
        });
        request.graduation_policy = policy;
        let call =
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()).unwrap();
        assert!(
            matches!(call.instruction, factory_core::Message::CreateFactoryPool { graduation_policy, .. } if graduation_policy == policy)
        );
        request.graduation_hook = None;
        assert_eq!(
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).is_ok(),
            policy == GraduationPolicy::default()
        );
    }
    for policy in [
        GraduationPolicy {
            launch_bps: 10001,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 10001,
        },
    ] {
        let mut request = creation();
        request.graduation_hook = Some(factory_core::GraduationHook {
            program_id: AccountId::new([19; 32]),
            configuration: vec![],
        });
        request.graduation_policy = policy;
        assert!(build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).is_err());
    }
}

#[test]
fn matching_active_sale_requires_the_same_graduation_split() {
    let mut state = valid_factory_state(sale());
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![],
    });
    state.graduation_policy = GraduationPolicy {
        launch_bps: 2500,
        collateral_bps: 7500,
    };
    let mut request = creation();
    request.graduation_hook = state.graduation_hook.clone();
    request.graduation_policy = state.graduation_policy;
    assert!(
        next_creation_invocation(
            sale(),
            AccountId::new([9; 32]),
            request.clone(),
            Some(&state)
        )
        .is_ok()
    );
    for policy in [
        GraduationPolicy {
            launch_bps: 2501,
            collateral_bps: 7500,
        },
        GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7501,
        },
    ] {
        request.graduation_policy = policy;
        assert!(matches!(
            next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            ),
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
}

#[test]
fn matching_active_sale_requires_the_same_graduation_reserve() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let mut state = valid_factory_state(sale);
    state.graduation_reserve = 120;
    state.total_supply += 120;
    let mut request = creation();
    assert!(matches!(
        next_creation_invocation(sale, creator, request.clone(), Some(&state)),
        Err(ClientError::LaunchParametersMismatch)
    ));
    request.graduation_reserve = 120;
    assert!(
        next_creation_invocation(sale, creator, request, Some(&state))
            .unwrap()
            .is_none()
    );
}

#[test]
fn curve_sdk_allows_short_sales_until_the_exclusive_end() {
    for now in [399, 400, 401] {
        let mut request = creation();
        request.ending_policy = factory_core::EndingPolicy::Either;
        request.end_timestamp = Some(400);
        request.now = now;
        assert_eq!(
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).is_ok(),
            now < 400,
            "hint {now}"
        );
    }
}
