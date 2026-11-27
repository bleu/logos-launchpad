use curve_core::{Message as CurveInstruction, compute_config_pda};
use lee::AccountId;
use lee_core::account::Actor;

use crate::PublicInvocation;
use crate::invocation::invocation;

fn namespace_actors(namespace: AccountId, curve: AccountId, signer: AccountId) -> Vec<Actor> {
    vec![
        Actor::new(signer, curve),
        Actor::new(compute_config_pda(namespace, curve), curve),
        Actor::new(
            curve_core::compute_admin_role(namespace, curve),
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        ),
        Actor::new(signer, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
    ]
}

pub fn build_initialize_namespace_invocation(
    namespace: AccountId,
    curve: AccountId,
    fee: u16,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(namespace, curve),
        namespace_actors(namespace, curve, namespace),
        vec![namespace],
        CurveInstruction::InitializeNamespace {
            namespace,
            protocol_fee_bps: fee.into(),
            treasury,
        },
    )
    .with_pda(curve, curve_core::compute_config_pda_seed(namespace))
    .with_pda(curve, curve_core::compute_admin_role_seed(namespace))
}
pub fn build_update_fee_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
    fee: u16,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(admin, curve),
        namespace_actors(namespace, curve, admin),
        vec![admin],
        CurveInstruction::UpdateFee {
            namespace,
            protocol_fee_bps: fee.into(),
        },
    )
}
pub fn build_update_treasury_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
    treasury: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(admin, curve),
        namespace_actors(namespace, curve, admin),
        vec![admin],
        CurveInstruction::UpdateTreasury {
            namespace,
            treasury,
        },
    )
}
pub fn build_renounce_admin_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
) -> PublicInvocation<CurveInstruction> {
    invocation(
        Actor::new(admin, curve),
        namespace_actors(namespace, curve, admin),
        vec![admin],
        CurveInstruction::RenounceAdmin { namespace },
    )
}
pub fn build_transfer_admin_invocation(
    namespace: AccountId,
    curve: AccountId,
    admin: AccountId,
    new_admin: AccountId,
    candidate: curve_core::AdminCandidate,
) -> PublicInvocation<CurveInstruction> {
    let mut actors = namespace_actors(namespace, curve, admin);
    actors.push(Actor::new(
        new_admin,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    let signers = match candidate {
        curve_core::AdminCandidate::Signer => vec![admin, new_admin],
        curve_core::AdminCandidate::Pda { program_id, .. } => {
            actors.push(Actor::new(new_admin, program_id));
            vec![admin]
        }
    };
    let successor_proof = match &candidate {
        curve_core::AdminCandidate::Pda { program_id, seed } => {
            let seed = lee_core::program::PdaSeed::new(*seed);
            (AccountId::for_public_pda(program_id, &seed) == new_admin)
                .then_some((*program_id, seed))
        }
        curve_core::AdminCandidate::Signer => None,
    };
    let call = invocation(
        Actor::new(admin, curve),
        actors,
        signers,
        CurveInstruction::TransferAdmin {
            namespace,
            new_admin,
            candidate,
        },
    );
    if let Some((program, seed)) = successor_proof {
        return call.with_pda(program, seed);
    }
    call
}
