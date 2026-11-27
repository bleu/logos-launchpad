use lee::AccountId;
use lee_core::account::Actor;

use crate::invocation::clock_actor;
use crate::{
    build_set_curve_paused_invocation, build_set_factory_paused_invocation,
    build_set_lbp_paused_invocation,
};

use super::fixtures::{CURVE_PROGRAM_ID, lbp_sale, sale};

#[test]
fn pause_builders_keep_current_role_signers_and_factory_relay_admission() {
    let creator = AccountId::new([9; 32]);
    let curve = build_set_factory_paused_invocation(sale(), creator, true);
    let lbp = build_set_lbp_paused_invocation(lbp_sale(), creator, true);
    for (call, namespace, salt, factory_program, engine, role) in [
        (
            &curve,
            sale().namespace,
            sale().launch_salt,
            sale().factory_program_id,
            sale().curve_program_id,
            factory_core::compute_curve_creator_role_pda(
                sale().namespace,
                sale().factory_program_id,
                sale().launch_salt,
            ),
        ),
        (
            &lbp,
            lbp_sale().namespace,
            lbp_sale().launch_salt,
            lbp_sale().factory_program_id,
            lbp_sale().lbp_program_id,
            factory_core::compute_creator_role_pda(
                lbp_sale().namespace,
                lbp_sale().factory_program_id,
                lbp_sale().launch_salt,
            ),
        ),
    ] {
        let factory = factory_core::compute_factory_pda(namespace, factory_program, salt);
        assert_eq!(call.destination, Actor::new(factory, factory_program));
        assert_eq!(call.signer_accounts, [creator]);
        assert_eq!(
            call.instruction,
            factory_core::Message::SetPaused {
                creator,
                paused: true
            }
        );
        assert!(call.actors.contains(&Actor::new(
            creator,
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
        )));
        for actor in [
            Actor::new(factory, engine),
            Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        ] {
            assert!(
                call.account_mentions()
                    .iter()
                    .any(|mention| mention.actor() == actor
                        && !mention.authorizes
                        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. }))
            );
        }
    }
}

#[test]
fn direct_pause_builder_uses_owner_role_and_signed_curve_envelope() {
    let owner = AccountId::new([9; 32]);
    let pool = AccountId::new([10; 32]);
    let call =
        build_set_curve_paused_invocation(sale().namespace, CURVE_PROGRAM_ID, pool, owner, false);
    assert_eq!(call.destination, Actor::new(owner, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [owner]);
    assert_eq!(
        call.instruction,
        curve_core::Message::Execute {
            pool,
            instruction: Box::new(curve_core::Message::SetPaused { paused: false })
        }
    );
    let role = curve_core::compute_pool_owner_role(sale().namespace, pool, CURVE_PROGRAM_ID);
    assert!(call.account_mentions().iter().any(|mention| mention.actor()
        == Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        && !mention.authorizes
        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. })));
    assert!(call.actors.contains(&clock_actor()));
}
