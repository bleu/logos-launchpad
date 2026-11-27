use super::{admin_call, emergency_key, fresh_sale, namespace_config, sale_with_pause, saved_pool};
use crate::fixture::{
    AUTHORITY, COLLATERAL, CURVE, FACTORY, FactoryFixture, LAUNCH, SALT, TREASURY, account,
    balance, curve_call, curve_call_at, execute, execute_at, holding, id, key,
    rejected_without_changes,
};
use lee::{Actor, PrivateKey};
use lez_compat_core::{
    authority,
    token::{TokenKind, TokenState},
};

fn trade(
    fixture: &mut FactoryFixture,
    message: curve_core::Message,
) -> Result<(), lee::error::LeeError> {
    curve_call_at(
        &mut fixture.state,
        &fixture.actors,
        fixture.pool,
        &key(49),
        message,
        fixture.inclusion_timestamp_ms,
    )
}

fn collect(fixture: &mut FactoryFixture) -> Result<(), lee::error::LeeError> {
    execute(
        &mut fixture.state,
        Actor::new(fixture.pool, CURVE),
        fixture.actors.clone(),
        curve_core::Message::CollectFees,
        &[],
    )
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn fresh_factory_sale_modes_apply_live_fees_and_collect_all_to_current_treasury_once() {
    for mode in [0, 1] {
        let mut fixture = fresh_sale(mode, None);
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 1_000,
            },
        )
        .unwrap();
        trade(
            &mut fixture,
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 82,
                token_in: COLLATERAL,
                now: 1,
            },
        )
        .unwrap();
        assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 900);
        assert_eq!(
            balance(&fixture.state, id(&key(49)), fixture.definition),
            82
        );
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 100);
        assert_eq!(saved_pool(&fixture).pool.real_reserve0, 318);
        assert_eq!(saved_pool(&fixture).pool.real_reserve1, 90);
        assert_eq!(saved_pool(&fixture).pool.fees_accrued, 10);
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 2_000,
            },
        )
        .unwrap();
        let sell = curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 82,
            min_amount_out: 72,
            token_in: fixture.definition,
            now: 1,
        };
        if mode == 0 {
            rejected_without_changes(
                &mut fixture.state,
                |state| curve_call(state, &fixture.actors, fixture.pool, &key(49), sell),
                "Restricted",
            );
        } else {
            trade(&mut fixture, sell).unwrap();
            assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 972);
            assert_eq!(balance(&fixture.state, id(&key(49)), fixture.definition), 0);
            assert_eq!(saved_pool(&fixture).pool.real_reserve0, 400);
            assert_eq!(saved_pool(&fixture).pool.real_reserve1, 0);
            assert_eq!(saved_pool(&fixture).pool.fees_accrued, 28);
        }
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateTreasury {
                namespace: fixture.namespace,
                treasury: id(&key(48)),
            },
        )
        .unwrap();
        for _ in 0..2 {
            collect(&mut fixture).unwrap();
            assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
            assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
            assert_eq!(
                balance(&fixture.state, id(&key(48)), COLLATERAL),
                if mode == 0 { 1_010 } else { 1_028 }
            );
            assert_eq!(
                balance(&fixture.state, fixture.pool, COLLATERAL),
                if mode == 0 { 90 } else { 0 }
            );
        }
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn failed_swap_and_fee_payout_restore_full_runtime_state_before_successful_retry() {
    let mut fixture = fresh_sale(1, None);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
    )
    .unwrap();
    let valid = fixture.state.clone();
    fixture.state = fixture.state.clone().with_public_accounts([account(
        holding(id(&key(49)), COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: id(&key(49)),
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            curve_call(
                state,
                &fixture.actors,
                fixture.pool,
                &key(49),
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 100,
                    min_amount_out: 82,
                    token_in: COLLATERAL,
                    now: 1,
                },
            )
        },
        "InsufficientBalance",
    );
    fixture.state = valid;
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 82,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    let valid = fixture.state.clone();
    fixture.state = fixture.state.clone().with_public_accounts([account(
        holding(TREASURY, COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: LAUNCH,
            owner: TREASURY,
            kind: TokenKind::Fungible,
            balance: 0,
        })
        .unwrap(),
    )]);
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            execute(
                state,
                Actor::new(fixture.pool, CURVE),
                fixture.actors.clone(),
                curve_core::Message::CollectFees,
                &[],
            )
        },
        "WrongDefinition",
    );
    fixture.state = valid;
    collect(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 900);
    assert_eq!(
        balance(&fixture.state, id(&key(49)), fixture.definition),
        82
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 90);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 10);
    assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn frozen_factory_sales_block_buys_and_sells_but_keep_admin_roles_and_fee_collection() {
    for mode in [0, 1] {
        let mut fixture = fresh_sale(mode, None);
        let emergency = emergency_key();
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &key(47),
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 1_000,
            },
        )
        .unwrap();
        trade(
            &mut fixture,
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 82,
                token_in: COLLATERAL,
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
        for message in [
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 100,
                min_amount_out: 1,
                token_in: COLLATERAL,
                now: 1,
            },
            curve_core::Message::SwapExactInput {
                membership: None,
                amount_in: 82,
                min_amount_out: 1,
                token_in: fixture.definition,
                now: 1,
            },
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 1,
                max_amount_in: 100,
                token_in: COLLATERAL,
                now: 1,
            },
        ] {
            rejected_without_changes(
                &mut fixture.state,
                |state| curve_call(state, &fixture.actors, fixture.pool, &key(49), message),
                "Frozen",
            );
        }
        for message in [
            curve_core::Message::UpdateFee {
                namespace: fixture.namespace,
                protocol_fee_bps: 2_000,
            },
            curve_core::Message::UpdateTreasury {
                namespace: fixture.namespace,
                treasury: id(&key(48)),
            },
        ] {
            admin_call(&mut fixture.state, &fixture.actors, &key(47), message).unwrap();
        }
        execute(
            &mut fixture.state,
            Actor::new(id(&key(48)), AUTHORITY),
            fixture.actors.clone(),
            authority::Message::Transfer {
                role: factory_core::compute_curve_creator_role_pda(
                    fixture.namespace,
                    FACTORY,
                    SALT,
                ),
                successor: id(&key(49)),
            },
            &[&key(48), &key(49)],
        )
        .unwrap();
        collect(&mut fixture).unwrap();
        assert_eq!(balance(&fixture.state, id(&key(48)), COLLATERAL), 1_010);
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 90);
        assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(49),
                    curve_core::Message::ClosePool { now: 1 },
                )
            },
            "Unauthorized",
        );
        admin_call(
            &mut fixture.state,
            &fixture.actors,
            &emergency,
            curve_core::Message::SetFrozen {
                namespace: fixture.namespace,
                frozen: false,
            },
        )
        .unwrap();
        if mode == 0 {
            trade(
                &mut fixture,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 100,
                    min_amount_out: 63,
                    token_in: COLLATERAL,
                    now: 1,
                },
            )
            .unwrap();
            assert_eq!(
                balance(&fixture.state, id(&key(49)), fixture.definition),
                145
            );
            assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 800);
        } else {
            let definition = fixture.definition;
            trade(
                &mut fixture,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 82,
                    min_amount_out: 72,
                    token_in: definition,
                    now: 1,
                },
            )
            .unwrap();
            assert_eq!(balance(&fixture.state, id(&key(49)), fixture.definition), 0);
            assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 972);
        }
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn buying_all_factory_d_closes_both_sale_modes_and_rejects_later_trades() {
    for mode in [0, 1] {
        let mut fixture = fresh_sale(mode, None);
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(48),
                    curve_core::Message::ClosePool { now: 1 },
                )
            },
            "Unauthorized",
        );
        trade(
            &mut fixture,
            curve_core::Message::SwapExactOutput {
                membership: None,
                amount_out: 400,
                max_amount_in: 667,
                token_in: COLLATERAL,
                now: 1,
            },
        )
        .unwrap();
        assert_eq!(
            saved_pool(&fixture).pool.lifecycle,
            pool::PoolLifecycle::Closed
        );
        assert_eq!(
            balance(&fixture.state, id(&key(49)), fixture.definition),
            400
        );
        assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 333);
        assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
        assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 667);
        assert_eq!(
            balance(&fixture.state, fixture.factory, fixture.definition),
            0
        );
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(49),
                    curve_core::Message::SwapExactInput {
                        membership: None,
                        amount_in: 1,
                        min_amount_out: 0,
                        token_in: COLLATERAL,
                        now: 1,
                    },
                )
            },
            "InvalidPool",
        );
        if mode == 1 {
            rejected_without_changes(
                &mut fixture.state,
                |state| {
                    curve_call(
                        state,
                        &fixture.actors,
                        fixture.pool,
                        &key(49),
                        curve_core::Message::SwapExactInput {
                            membership: None,
                            amount_in: 1,
                            min_amount_out: 0,
                            token_in: fixture.definition,
                            now: 1,
                        },
                    )
                },
                "InvalidPool",
            );
        }
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn pool_treasury_alias_collects_fees_without_self_transfer_or_reserve_change() {
    let mut fixture = fresh_sale(1, None);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
    )
    .unwrap();
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 82,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateTreasury {
            namespace: fixture.namespace,
            treasury: fixture.pool,
        },
    )
    .unwrap();
    collect(&mut fixture).unwrap();
    assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
    assert_eq!(saved_pool(&fixture).pool.real_reserve1, 90);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 100);
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 0);
    let collected = fixture.state.clone();
    collect(&mut fixture).unwrap();
    assert!(
        fixture.state == collected,
        "zero-fee retry changed runtime state"
    );
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn factory_expiry_rejects_current_and_stale_trades_and_can_close_while_frozen() {
    let mut fixture = fresh_sale(1, Some(301));
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 90,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    fixture.inclusion_timestamp_ms = 301_000;
    for now in [1, 301] {
        for token_in in [COLLATERAL, fixture.definition] {
            let before = fixture.state.clone();
            let result = trade(
                &mut fixture,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 10,
                    min_amount_out: 1,
                    token_in,
                    now,
                },
            );
            if now == 1 {
                assert!(
                    matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
                    "{result:?}"
                );
            } else {
                assert!(
                    matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("InvalidPool")),
                    "{result:?}"
                );
            }
            assert!(
                fixture.state == before,
                "expired trade changed runtime state or nonce"
            );
        }
    }
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency_key(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
    )
    .unwrap();
    for _ in 0..2 {
        fixture
            .call(
                &key(48),
                factory_core::Message::WithdrawFactoryProceeds {
                    creator: id(&key(48)),
                    now: 301,
                },
            )
            .unwrap();
    }
    assert_eq!(
        saved_pool(&fixture).pool.lifecycle,
        pool::PoolLifecycle::Withdrawn
    );
    assert_eq!(balance(&fixture.state, fixture.pool, fixture.definition), 0);
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(
        balance(&fixture.state, fixture.factory, fixture.definition),
        310
    );
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 100);
    assert_eq!(
        balance(&fixture.state, id(&key(49)), fixture.definition),
        90
    );
    assert_eq!(balance(&fixture.state, id(&key(49)), COLLATERAL), 900);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency_key(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: false,
        },
    )
    .unwrap();
    for token_in in [COLLATERAL, fixture.definition] {
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &key(49),
                    curve_core::Message::SwapExactInput {
                        membership: None,
                        amount_in: 1,
                        min_amount_out: 0,
                        token_in,
                        now: 301,
                    },
                )
            },
            "InvalidPool",
        );
    }
}

fn set_sale_paused(
    fixture: &mut FactoryFixture,
    signer: &PrivateKey,
    paused: bool,
) -> Result<(), lee::error::LeeError> {
    fixture.call(
        signer,
        factory_core::Message::SetPaused {
            creator: id(signer),
            paused,
        },
    )
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn curve_pause_uses_current_creator_blocks_all_swaps_and_preserves_frozen_settlement() {
    let mut fixture = sale_with_pause(1, Some(301), curve_core::PausePolicy::AuthorityControlled);
    let creator = key(48);
    let successor = key(49);
    fixture.actors.extend([
        Actor::new(id(&emergency_key()), AUTHORITY),
        Actor::new(id(&emergency_key()), FACTORY),
    ]);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &key(47),
        curve_core::Message::UpdateFee {
            namespace: fixture.namespace,
            protocol_fee_bps: 1_000,
        },
    )
    .unwrap();
    trade(
        &mut fixture,
        curve_core::Message::SwapExactInput {
            membership: None,
            amount_in: 100,
            min_amount_out: 82,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    execute(
        &mut fixture.state,
        Actor::new(id(&creator), AUTHORITY),
        fixture.actors.clone(),
        authority::Message::Transfer {
            role: factory_core::compute_curve_creator_role_pda(fixture.namespace, FACTORY, SALT),
            successor: id(&successor),
        },
        &[&creator, &successor],
    )
    .unwrap();
    for paused in [false, true, true] {
        for signer in [&creator, &key(47), &emergency_key()] {
            let before = fixture.state.clone();
            let result = set_sale_paused(&mut fixture, signer, paused);
            assert!(
                matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Unauthorized")),
                "{result:?}"
            );
            assert!(fixture.state == before);
        }
        rejected_without_changes(
            &mut fixture.state,
            |state| {
                curve_call(
                    state,
                    &fixture.actors,
                    fixture.pool,
                    &successor,
                    curve_core::Message::SetPaused { paused },
                )
            },
            "Unauthorized",
        );
        set_sale_paused(&mut fixture, &successor, paused).unwrap();
    }
    assert_eq!(
        saved_pool(&fixture).trade_control,
        curve_core::TradeControl::Paused
    );
    for token_in in [COLLATERAL, fixture.definition] {
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
                &mut fixture.state,
                |state| {
                    curve_call(
                        state,
                        &fixture.actors,
                        fixture.pool,
                        &successor,
                        instruction,
                    )
                },
                "Paused",
            );
        }
    }
    collect(&mut fixture).unwrap();
    assert_eq!(balance(&fixture.state, TREASURY, COLLATERAL), 10);
    assert_eq!(saved_pool(&fixture).pool.fees_accrued, 0);
    admin_call(
        &mut fixture.state,
        &fixture.actors,
        &emergency_key(),
        curve_core::Message::SetFrozen {
            namespace: fixture.namespace,
            frozen: true,
        },
    )
    .unwrap();
    set_sale_paused(&mut fixture, &successor, false).unwrap();
    assert!(
        namespace_config(&fixture.state, fixture.namespace)
            .emergency
            .is_frozen
    );
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            curve_call(
                state,
                &fixture.actors,
                fixture.pool,
                &successor,
                curve_core::Message::SwapExactInput {
                    membership: None,
                    amount_in: 1,
                    min_amount_out: 0,
                    token_in: COLLATERAL,
                    now: 1,
                },
            )
        },
        "Frozen",
    );
    set_sale_paused(&mut fixture, &successor, true).unwrap();
    fixture.inclusion_timestamp_ms = 301_000;
    let before = fixture.state.clone();
    let result = set_sale_paused(&mut fixture, &successor, false);
    assert!(
        matches!(result, Err(lee::error::LeeError::OutOfValidityWindow)),
        "{result:?}"
    );
    assert!(fixture.state == before);
    for _ in 0..3 {
        fixture
            .call(
                &successor,
                factory_core::Message::WithdrawFactoryProceeds {
                    creator: id(&successor),
                    now: 301,
                },
            )
            .unwrap();
    }
    assert_eq!(
        saved_pool(&fixture).trade_control,
        curve_core::TradeControl::Paused
    );
    assert_eq!(
        saved_pool(&fixture).pool.lifecycle,
        pool::PoolLifecycle::Withdrawn
    );
    assert_eq!(balance(&fixture.state, fixture.pool, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, fixture.factory, COLLATERAL), 0);
    assert_eq!(balance(&fixture.state, id(&successor), COLLATERAL), 990);
    rejected_without_changes(
        &mut fixture.state,
        |state| {
            execute_at(
                state,
                Actor::new(fixture.factory, FACTORY),
                fixture.actors.clone(),
                factory_core::Message::SetPaused {
                    creator: id(&successor),
                    paused: false,
                },
                &[&successor],
                301_000,
            )
        },
        "Closed",
    );
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn disabled_curve_pause_and_depleted_sale_controls_reject_without_changes() {
    let mut disabled = fresh_sale(1, None);
    let before = disabled.state.clone();
    let result = set_sale_paused(&mut disabled, &key(48), true);
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("PauseDisabled")),
        "{result:?}"
    );
    assert!(disabled.state == before);
    let mut depleted = sale_with_pause(1, None, curve_core::PausePolicy::AuthorityControlled);
    trade(
        &mut depleted,
        curve_core::Message::SwapExactOutput {
            membership: None,
            amount_out: 400,
            max_amount_in: 667,
            token_in: COLLATERAL,
            now: 1,
        },
    )
    .unwrap();
    for paused in [false, true] {
        let before = depleted.state.clone();
        let result = set_sale_paused(&mut depleted, &key(48), paused);
        assert!(
            matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(reason)) if reason.contains("Closed")),
            "{result:?}"
        );
        assert!(depleted.state == before);
    }
}
