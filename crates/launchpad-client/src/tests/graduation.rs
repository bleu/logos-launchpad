use lee::AccountId;
use lee_core::account::Actor;

use crate::curve::factory_pool_addresses;
use crate::invocation::{associated_token_account, clock_actor, token_actor};
use crate::lbp::lbp_addresses;
use crate::{
    ClientError, GraduationTarget, build_create_fixed_price_invocation,
    build_create_weighted_invocation, build_graduation_invocation,
};

use super::fixtures::{
    fixed_creation, lbp_sale, sale, valid_factory_state, valid_lbp_factory_state, weighted_creation,
};

#[test]
fn graduation_admits_adapter_custody_and_its_payout_holdings_without_authorization() {
    let mut state = valid_factory_state(sale());
    let adapter = AccountId::new([12; 32]);
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: adapter,
        configuration: vec![],
    });
    let call = build_graduation_invocation(
        GraduationTarget::Curve {
            sale: sale(),
            state: &state,
            creator_hint: AccountId::new([9; 32]),
        },
        AccountId::new([10; 32]),
    )
    .unwrap();
    let a = factory_pool_addresses(sale());
    let custody = factory_core::compute_graduation_pda(adapter, a.factory);
    for account in [
        custody,
        associated_token_account(custody, a.token_definition),
        associated_token_account(custody, sale().collateral_definition),
    ] {
        assert!(
            call.account_mentions()
                .iter()
                .any(|m| m.actor().account_id == account
                    && !m.authorizes
                    && matches!(m.identity, wallet::AccountIdentity::PublicPda { .. }))
        );
    }
}

#[test]
fn graduation_builder_uses_typed_callback_and_adapter_custody() {
    let mut state = valid_factory_state(sale());
    let adapter = AccountId::new([9; 32]);
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: adapter,
        configuration: vec![7, 11],
    });
    let call = build_graduation_invocation(
        GraduationTarget::Curve {
            sale: sale(),
            state: &state,
            creator_hint: AccountId::new([9; 32]),
        },
        AccountId::new([4; 32]),
    )
    .unwrap();
    assert!(call.signer_accounts.is_empty());
    assert!(call.actors.contains(&Actor::new(
        factory_core::compute_graduation_pda(adapter, factory_pool_addresses(sale()).factory),
        adapter
    )));
    assert_eq!(
        call.instruction,
        factory_core::Message::ExecuteGraduationHook {
            creator_hint: Some(AccountId::new([9; 32]))
        }
    );
}

#[test]
fn lbp_graduation_admits_setup_callback_and_custody_without_creator_authorization() {
    let sale = lbp_sale();
    let a = lbp_addresses(sale);
    for weighted in [false, true] {
        let state = valid_lbp_factory_state(weighted);
        for creator_hint in [AccountId::new([21; 32]), AccountId::new([22; 32])] {
            let treasury = AccountId::new([23; 32]);
            let call = build_graduation_invocation(
                GraduationTarget::Lbp {
                    sale,
                    state: &state,
                    creator_hint,
                },
                treasury,
            )
            .unwrap();
            assert_eq!(
                call.destination,
                Actor::new(a.factory, sale.factory_program_id)
            );
            assert!(call.signer_accounts.is_empty());
            assert_eq!(
                call.instruction,
                factory_core::Message::ExecuteGraduationHook {
                    creator_hint: Some(creator_hint)
                }
            );
            assert!(
                call.account_mentions()
                    .iter()
                    .all(|mention| !mention.authorizes)
            );
            let custody_program = state.graduation_hook.as_ref().unwrap().program_id;
            let custody = factory_core::compute_graduation_pda(custody_program, a.factory);
            for actor in [
                Actor::new(
                    state.creator_role_id,
                    curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                ),
                Actor::new(creator_hint, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
                Actor::new(a.factory, sale.lbp_program_id),
                Actor::new(a.pool, sale.lbp_program_id),
                Actor::new(custody, custody_program),
                Actor::new(
                    curve_core::compute_config_pda(sale.namespace, sale.namespace_program_id),
                    sale.namespace_program_id,
                ),
                clock_actor(),
                token_actor(a.token_definition),
                token_actor(sale.collateral_definition),
            ] {
                assert!(call.actors.contains(&actor), "missing {actor:?}");
            }
            let mentions = call.account_mentions();
            for owner in [
                a.factory,
                a.pool,
                a.creator_escrow,
                creator_hint,
                custody,
                treasury,
            ] {
                for definition in [a.token_definition, sale.collateral_definition] {
                    let holding = token_actor(associated_token_account(owner, definition));
                    assert!(mentions.iter().any(|m| m.actor() == holding
                        && !m.authorizes
                        && matches!(m.identity, wallet::AccountIdentity::PublicPda { .. })));
                }
            }
            assert_eq!(
                call.actors.len(),
                call.actors
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
            );
        }
    }
}

#[test]
fn lbp_graduation_rejects_foreign_identity_and_requires_a_saved_hook() {
    let sale = lbp_sale();
    let hint = AccountId::new([21; 32]);
    let treasury = AccountId::new([23; 32]);
    for weighted in [false, true] {
        let state = valid_lbp_factory_state(weighted);
        for index in 0..9 {
            let mut bad = state.clone();
            let foreign = AccountId::new([99; 32]);
            match index {
                0 => bad.namespace = foreign,
                1 => bad.launch_salt = [99; 32],
                2 => bad.namespace_program_id = foreign,
                3 => bad.lbp_program_id = foreign,
                4 => bad.collateral_definition_id = foreign,
                5 => bad.token_definition_id = foreign,
                6 => bad.creator_escrow_id = foreign,
                7 => bad.pool_id = foreign,
                _ => bad.creator_role_id = foreign,
            }
            assert!(matches!(
                build_graduation_invocation(
                    GraduationTarget::Lbp {
                        sale,
                        state: &bad,
                        creator_hint: hint,
                    },
                    treasury
                ),
                Err(ClientError::FactorySaleMismatch)
            ));
        }
        let mut no_hook = state;
        no_hook.graduation_hook = None;
        assert!(matches!(
            build_graduation_invocation(
                GraduationTarget::Lbp {
                    sale,
                    state: &no_hook,
                    creator_hint: hint,
                },
                treasury
            ),
            Err(ClientError::GraduationNotConfigured)
        ));
    }
}

#[test]
fn typed_lbp_creation_keeps_the_complete_immutable_hook() {
    let hook = factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![3, 7, 11],
    };
    let mut fixed = fixed_creation();
    fixed.graduation_hook = Some(hook.clone());
    let fixed_call =
        build_create_fixed_price_invocation(lbp_sale().factory_program_id, fixed.clone());
    assert_eq!(
        fixed_call.instruction,
        factory_core::Message::CreateFixedPriceLbp { inputs: fixed }
    );
    let mut weighted = weighted_creation();
    weighted.graduation_hook = Some(hook);
    let weighted_call =
        build_create_weighted_invocation(lbp_sale().factory_program_id, weighted.clone());
    assert_eq!(
        weighted_call.instruction,
        factory_core::Message::CreateWeightedLbp { inputs: weighted }
    );
}

#[test]
fn curve_graduation_admits_both_current_creator_holdings_without_signing() {
    let mut state = valid_factory_state(sale());
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![],
    });
    for creator_hint in [AccountId::new([21; 32]), AccountId::new([22; 32])] {
        let call = build_graduation_invocation(
            GraduationTarget::Curve {
                sale: sale(),
                state: &state,
                creator_hint,
            },
            AccountId::new([23; 32]),
        )
        .unwrap();
        assert!(call.signer_accounts.is_empty());
        assert_eq!(
            call.instruction,
            factory_core::Message::ExecuteGraduationHook {
                creator_hint: Some(creator_hint)
            }
        );
        assert!(call.actors.contains(&Actor::new(
            creator_hint,
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
        )));
        for definition in [state.token_definition_id, sale().collateral_definition] {
            let holding = associated_token_account(creator_hint, definition);
            assert!(
                call.account_mentions()
                    .iter()
                    .any(|mention| mention.actor() == token_actor(holding)
                        && !mention.authorizes
                        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. }))
            );
        }
    }
}
