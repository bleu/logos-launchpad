use factory_core::{FactoryState, Message as FactoryInstruction};
use lee::AccountId;
use lee_core::account::Actor;

use crate::curve::{
    admit_factory_accounts, factory_actors, factory_pool_addresses, validate_factory_state,
};
use crate::invocation::{associated_token_account, invocation, token_actor};
use crate::lbp::{lbp_addresses, lbp_setup_invocation, validate_lbp_factory_state};
use crate::{ClientError, FactorySale, LbpSale, PublicInvocation, Result};

/// Persisted terms select the adapter. A creator hint only admits the resolved payout accounts.
#[derive(Debug, Clone, Copy)]
pub enum GraduationTarget<'a> {
    Curve {
        sale: FactorySale,
        state: &'a FactoryState,
        creator_hint: AccountId,
    },
    Lbp {
        sale: LbpSale,
        state: &'a factory_core::LbpLaunchState,
        creator_hint: AccountId,
    },
}

/// The callback has a fixed typed context and no arbitrary account extension.
pub fn build_graduation_invocation(
    target: GraduationTarget<'_>,
    treasury: AccountId,
) -> Result<PublicInvocation<FactoryInstruction>> {
    let (mut call, factory, hook, definition, collateral) = match target {
        GraduationTarget::Curve {
            sale,
            state,
            creator_hint,
        } => {
            validate_factory_state(sale, state)?;
            let hook = state
                .graduation_hook
                .as_ref()
                .ok_or(ClientError::GraduationNotConfigured)?;
            let a = factory_pool_addresses(sale);
            let call = admit_factory_accounts(
                invocation(
                    Actor::new(a.factory, sale.factory_program_id),
                    factory_actors(sale, creator_hint, Some(treasury)),
                    vec![],
                    FactoryInstruction::ExecuteGraduationHook {
                        creator_hint: Some(creator_hint),
                    },
                ),
                sale,
            )
            .with_holding(creator_hint, a.token_definition)
            .with_holding(creator_hint, sale.collateral_definition);
            (
                call,
                a.factory,
                hook,
                a.token_definition,
                sale.collateral_definition,
            )
        }
        GraduationTarget::Lbp {
            sale,
            state,
            creator_hint,
        } => {
            validate_lbp_factory_state(sale, state)?;
            let hook = state
                .graduation_hook
                .as_ref()
                .ok_or(ClientError::GraduationNotConfigured)?;
            let a = lbp_addresses(sale);
            let mut call = lbp_setup_invocation(
                sale,
                creator_hint,
                FactoryInstruction::ExecuteGraduationHook {
                    creator_hint: Some(creator_hint),
                },
            );
            call.signer_accounts.clear();
            (
                call,
                a.factory,
                hook,
                a.token_definition,
                sale.collateral_definition,
            )
        }
    };
    let custody = factory_core::compute_graduation_pda(hook.program_id, factory);
    call.actors.push(Actor::new(custody, hook.program_id));
    call.actors.push(token_actor(custody));
    for owner in [custody, treasury] {
        for token in [definition, collateral] {
            call.actors
                .push(token_actor(associated_token_account(owner, token)));
            call = call.with_holding(owner, token);
        }
    }
    let mut seen = std::collections::HashSet::new();
    call.actors.retain(|actor| seen.insert(*actor));
    Ok(call.with_pda(
        hook.program_id,
        factory_core::compute_graduation_seed(factory),
    ))
}
