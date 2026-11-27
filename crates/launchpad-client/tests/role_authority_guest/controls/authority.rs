use super::{admin_call, emergency_key, fresh_sale, namespace_config};
use crate::fixture::{
    AUTHORITY, COLLATERAL, CURVE, LAUNCH, TOKEN, TREASURY, account, balance, base_state,
    curve_call, execute, holding, id, key, rejected_without_changes,
};
use lee::{AccountId, Actor};
use lez_compat_core::authority;

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn namespace_admin_updates_fee_and_treasury_with_validation_and_authority_checks() {
    let namespace = key(60);
    let outsider = key(61);
    let namespace_id = id(&namespace);
    let (mut state, actors) = base_state(&namespace, &[id(&outsider)]);
    for message in [
        curve_core::Message::UpdateFee {
            namespace: namespace_id,
            protocol_fee_bps: 250,
        },
        curve_core::Message::UpdateTreasury {
            namespace: namespace_id,
            treasury: id(&outsider),
        },
    ] {
        rejected_without_changes(
            &mut state,
            |state| admin_call(state, &actors, &outsider, message.clone()),
            "Unauthorized",
        );
        admin_call(&mut state, &actors, &namespace, message).unwrap();
    }
    let config = namespace_config(&state, namespace_id);
    assert_eq!(config.protocol_fee_bps, 250);
    assert_eq!(config.treasury, id(&outsider));
    for (message, reason) in [
        (
            curve_core::Message::UpdateFee {
                namespace: namespace_id,
                protocol_fee_bps: 10_000,
            },
            "InvalidFee",
        ),
        (
            curve_core::Message::UpdateTreasury {
                namespace: namespace_id,
                treasury: AccountId::default(),
            },
            "InvalidTreasury",
        ),
    ] {
        rejected_without_changes(
            &mut state,
            |state| admin_call(state, &actors, &namespace, message),
            reason,
        );
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn signer_admin_transfer_requires_consent_and_renunciation_keeps_emergency_and_other_namespace() {
    let namespace = key(60);
    let successor = key(61);
    let other = key(62);
    let emergency = emergency_key();
    let namespace_id = id(&namespace);
    let (mut state, mut actors) =
        base_state(&namespace, &[id(&successor), id(&other), id(&emergency)]);
    actors.extend([
        Actor::new(id(&other), CURVE),
        Actor::new(id(&other), AUTHORITY),
        Actor::new(curve_core::compute_config_pda(id(&other), CURVE), CURVE),
        Actor::new(curve_core::compute_admin_role(id(&other), CURVE), AUTHORITY),
    ]);
    admin_call(
        &mut state,
        &actors,
        &other,
        curve_core::Message::InitializeNamespace {
            namespace: id(&other),
            protocol_fee_bps: 50,
            treasury: TREASURY,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: namespace_id,
            frozen: true,
        },
    )
    .unwrap();
    let transfer = curve_core::Message::TransferAdmin {
        namespace: namespace_id,
        new_admin: id(&successor),
        candidate: curve_core::AdminCandidate::Signer,
    };
    state = state.with_public_accounts([account(Actor::new(id(&successor), AUTHORITY), vec![])]);
    rejected_without_changes(
        &mut state,
        |state| admin_call(state, &actors, &namespace, transfer.clone()),
        "Unauthorized",
    );
    execute(
        &mut state,
        Actor::new(namespace_id, CURVE),
        actors.clone(),
        transfer,
        &[&namespace, &successor],
    )
    .unwrap();
    for signer in [&namespace, &other] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    signer,
                    curve_core::Message::UpdateFee {
                        namespace: namespace_id,
                        protocol_fee_bps: 100,
                    },
                )
            },
            "Unauthorized",
        );
    }
    admin_call(
        &mut state,
        &actors,
        &successor,
        curve_core::Message::UpdateFee {
            namespace: namespace_id,
            protocol_fee_bps: 100,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &successor,
        curve_core::Message::RenounceAdmin {
            namespace: namespace_id,
        },
    )
    .unwrap();
    for signer in [&namespace, &successor, &emergency] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    signer,
                    curve_core::Message::UpdateTreasury {
                        namespace: namespace_id,
                        treasury: id(&successor),
                    },
                )
            },
            "Renounced",
        );
    }
    let config = namespace_config(&state, namespace_id);
    assert_eq!(config.protocol_fee_bps, 100);
    assert_eq!(config.treasury, TREASURY);
    assert!(config.emergency.is_frozen);
    assert_eq!(config.emergency.authority(), id(&emergency));
    let other_config = namespace_config(&state, id(&other));
    assert_eq!(other_config.protocol_fee_bps, 50);
    assert!(!other_config.emergency.is_frozen);
    admin_call(
        &mut state,
        &actors,
        &other,
        curve_core::Message::UpdateFee {
            namespace: id(&other),
            protocol_fee_bps: 200,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: namespace_id,
            frozen: false,
        },
    )
    .unwrap();
    assert!(!namespace_config(&state, namespace_id).emergency.is_frozen);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn emergency_transfer_preserves_freeze_and_all_noop_calls_require_current_authority() {
    let namespace = key(60);
    let successor = key(61);
    let emergency = emergency_key();
    let ns = id(&namespace);
    let (mut state, actors) = base_state(&namespace, &[id(&successor), id(&emergency)]);
    assert!(!namespace_config(&state, ns).emergency.is_frozen);
    for frozen in [false, true, true] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    &namespace,
                    curve_core::Message::SetFrozen {
                        namespace: ns,
                        frozen,
                    },
                )
            },
            "Unauthorized",
        );
        admin_call(
            &mut state,
            &actors,
            &emergency,
            curve_core::Message::SetFrozen {
                namespace: ns,
                frozen,
            },
        )
        .unwrap();
    }
    for new_authority in [id(&emergency), id(&successor)] {
        rejected_without_changes(
            &mut state,
            |state| {
                admin_call(
                    state,
                    &actors,
                    &namespace,
                    curve_core::Message::TransferEmergencyAuthority {
                        namespace: ns,
                        new_authority,
                    },
                )
            },
            "Unauthorized",
        );
        admin_call(
            &mut state,
            &actors,
            &emergency,
            curve_core::Message::TransferEmergencyAuthority {
                namespace: ns,
                new_authority,
            },
        )
        .unwrap();
    }
    let config = namespace_config(&state, ns);
    assert!(config.emergency.is_frozen);
    assert_eq!(config.emergency.authority(), id(&successor));
    for message in [
        curve_core::Message::SetFrozen {
            namespace: ns,
            frozen: true,
        },
        curve_core::Message::TransferEmergencyAuthority {
            namespace: ns,
            new_authority: id(&successor),
        },
    ] {
        rejected_without_changes(
            &mut state,
            |state| admin_call(state, &actors, &emergency, message),
            "Unauthorized",
        );
    }
    rejected_without_changes(
        &mut state,
        |state| {
            admin_call(
                state,
                &actors,
                &successor,
                curve_core::Message::TransferEmergencyAuthority {
                    namespace: ns,
                    new_authority: AccountId::default(),
                },
            )
        },
        "Unauthorized",
    );
    admin_call(
        &mut state,
        &actors,
        &successor,
        curve_core::Message::TransferEmergencyAuthority {
            namespace: ns,
            new_authority: id(&successor),
        },
    )
    .unwrap();
    for frozen in [false, false] {
        admin_call(
            &mut state,
            &actors,
            &successor,
            curve_core::Message::SetFrozen {
                namespace: ns,
                frozen,
            },
        )
        .unwrap();
    }
    assert!(!namespace_config(&state, ns).emergency.is_frozen);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn namespace_freeze_and_emergency_transfer_do_not_control_another_namespace_pool() {
    let mut fixture = fresh_sale(1, None);
    let other = key(63);
    let owner = key(49);
    let emergency = emergency_key();
    let other_id = id(&other);
    let other_pool = curve_core::compute_pool_pda(other_id, CURVE, LAUNCH, COLLATERAL, id(&owner));
    fixture.actors.extend([
        Actor::new(other_id, CURVE),
        Actor::new(other_id, AUTHORITY),
        Actor::new(curve_core::compute_config_pda(other_id, CURVE), CURVE),
        Actor::new(curve_core::compute_admin_role(other_id, CURVE), AUTHORITY),
        Actor::new(other_pool, CURVE),
        Actor::new(other_pool, TOKEN),
        Actor::new(
            curve_core::compute_pool_owner_role(other_id, other_pool, CURVE),
            AUTHORITY,
        ),
        holding(other_pool, LAUNCH),
        holding(other_pool, COLLATERAL),
    ]);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &other,
        curve_core::Message::InitializeNamespace {
            namespace: other_id,
            protocol_fee_bps: 0,
            treasury: TREASURY,
        },
    )
    .unwrap();
    curve_call(
        &mut fixture.state,
        &fixture.actors,
        other_pool,
        &owner,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            namespace: other_id,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: false,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&owner),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
    )
    .unwrap();
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency,
        curve_core::Message::TransferEmergencyAuthority {
            namespace: fixture.namespace,
            new_authority: id(&owner),
        },
    )
    .unwrap();
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            admin_call(
                state,
                &fixture.actors,
                &owner,
                curve_core::Message::SetFrozen {
                    namespace: other_id,
                    frozen: true,
                },
            )
        },
        "Unauthorized",
    );
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            admin_call(
                state,
                &fixture.actors,
                &other,
                curve_core::Message::UpdateFee {
                    namespace: fixture.namespace,
                    protocol_fee_bps: 100,
                },
            )
        },
        "Unauthorized",
    );
    assert!(
        namespace_config(&fixture.state, fixture.namespace)
            .emergency
            .is_frozen
    );
    assert!(
        !namespace_config(&fixture.state, other_id)
            .emergency
            .is_frozen
    );
    assert_eq!(
        namespace_config(&fixture.state, other_id)
            .emergency
            .authority(),
        id(&emergency)
    );
    curve_call(
        &mut fixture.state,
        &fixture.actors,
        other_pool,
        &owner,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 90,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    assert_eq!(balance(&fixture.state, other_pool, LAUNCH), 310);
    assert_eq!(balance(&fixture.state, other_pool, COLLATERAL), 200);
    assert_eq!(balance(&fixture.state, id(&owner), LAUNCH), 690);
    assert_eq!(balance(&fixture.state, id(&owner), COLLATERAL), 800);
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        400
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn frozen_direct_pool_keeps_owner_close_and_reserve_recovery_checks() {
    let namespace = key(60);
    let owner = key(61);
    let emergency = emergency_key();
    let ns = id(&namespace);
    let (mut state, mut actors) = base_state(&namespace, &[id(&owner), id(&emergency)]);
    let pool_id = curve_core::compute_pool_pda(ns, CURVE, LAUNCH, COLLATERAL, id(&owner));
    actors.extend([
        Actor::new(pool_id, CURVE),
        Actor::new(pool_id, TOKEN),
        Actor::new(
            curve_core::compute_pool_owner_role(ns, pool_id, CURVE),
            AUTHORITY,
        ),
        holding(pool_id, LAUNCH),
        holding(pool_id, COLLATERAL),
    ]);
    curve_call(
        &mut state,
        &actors,
        pool_id,
        &owner,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            namespace: ns,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: false,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&owner),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    admin_call(
        &mut state,
        &actors,
        &emergency,
        curve_core::Message::SetFrozen {
            namespace: ns,
            frozen: true,
        },
    )
    .unwrap();
    for instruction in [
        curve_core::Message::ClosePool { now: 1 },
        curve_core::Message::WithdrawReserves { now: 1 },
    ] {
        rejected_without_changes(
            &mut state,
            |state| curve_call(state, &actors, pool_id, &namespace, instruction.clone()),
            "Unauthorized",
        );
        curve_call(&mut state, &actors, pool_id, &owner, instruction).unwrap();
    }
    assert!(namespace_config(&state, ns).emergency.is_frozen);
    assert_eq!(balance(&state, id(&owner), LAUNCH), 1_000);
    assert_eq!(balance(&state, id(&owner), COLLATERAL), 1_000);
    assert_eq!(balance(&state, pool_id, LAUNCH), 0);
    assert_eq!(balance(&state, pool_id, COLLATERAL), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn direct_curve_pause_follows_transferred_owner_and_keeps_reserve_recovery() {
    let namespace = key(60);
    let owner = key(61);
    let successor = key(62);
    let emergency = emergency_key();
    let ns = id(&namespace);
    let (mut state, mut actors) =
        base_state(&namespace, &[id(&owner), id(&successor), id(&emergency)]);
    let pool = curve_core::compute_pool_pda(ns, CURVE, LAUNCH, COLLATERAL, id(&owner));
    let role = curve_core::compute_pool_owner_role(ns, pool, CURVE);
    actors.extend([
        Actor::new(pool, CURVE),
        Actor::new(pool, TOKEN),
        Actor::new(role, AUTHORITY),
        holding(pool, LAUNCH),
        holding(pool, COLLATERAL),
    ]);
    curve_call(
        &mut state,
        &actors,
        pool,
        &owner,
        curve_core::Message::CreatePool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::AuthorityControlled,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            namespace: ns,
            token0_definition_id: LAUNCH,
            token1_definition_id: COLLATERAL,
            allowed_input_token: None,
            defer_funding: false,
            token0_amount: 400,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 1_000,
            close_timestamp: None,
            close_on_depletion: None,
            owner: id(&owner),
            owner_program_id: None,
            owner_program_seed: None,
            curve_program_id: CURVE,
            now: 1,
        },
    )
    .unwrap();
    execute(
        &mut state,
        Actor::new(id(&owner), AUTHORITY),
        actors.clone(),
        authority::Message::Transfer {
            role,
            successor: id(&successor),
        },
        &[&owner, &successor],
    )
    .unwrap();
    for paused in [false, true, true] {
        for signer in [&owner, &namespace, &emergency] {
            rejected_without_changes(
                &mut state,
                |state| {
                    curve_call(
                        state,
                        &actors,
                        pool,
                        signer,
                        curve_core::Message::SetPaused { paused },
                    )
                },
                "Unauthorized",
            );
        }
        let call = launchpad_client::build_set_curve_paused_invocation(
            ns,
            CURVE,
            pool,
            id(&successor),
            paused,
        );
        execute(
            &mut state,
            call.destination,
            call.actors,
            call.instruction,
            &[&successor],
        )
        .unwrap();
    }
    for token_in in [LAUNCH, COLLATERAL] {
        for instruction in [
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 1,
                min_amount_out: 0,
                token_in,
                now: 1,
            },
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 1,
                max_amount_in: 100,
                token_in,
                now: 1,
            },
        ] {
            rejected_without_changes(
                &mut state,
                |state| curve_call(state, &actors, pool, &successor, instruction),
                "Paused",
            );
        }
    }
    for instruction in [
        curve_core::Message::ClosePool { now: 1 },
        curve_core::Message::WithdrawReserves { now: 1 },
    ] {
        curve_call(&mut state, &actors, pool, &successor, instruction).unwrap();
    }
    assert_eq!(balance(&state, id(&successor), LAUNCH), 1_400);
    assert_eq!(balance(&state, id(&successor), COLLATERAL), 1_100);
    assert_eq!(balance(&state, pool, LAUNCH), 0);
    assert_eq!(balance(&state, pool, COLLATERAL), 0);
    rejected_without_changes(
        &mut state,
        |state| {
            curve_call(
                state,
                &actors,
                pool,
                &successor,
                curve_core::Message::SetPaused { paused: false },
            )
        },
        "Closed",
    );
}
