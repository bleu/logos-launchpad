use factory_core::SettlementStage;
use lee::AccountId;
use lee_core::account::Actor;

use crate::curve::factory_pool_addresses;
use crate::invocation::{associated_token_account, token_actor};
use crate::{
    PoolFeeTarget, build_claim_creator_allocation_invocation, build_collect_fees_invocation,
    build_withdraw_factory_proceeds_invocation,
};

use super::fixtures::{CURVE_PROGRAM_ID, FACTORY_PROGRAM_ID, sale, valid_factory_state};

#[test]
fn settlement_admits_escrow_and_current_creator_and_treasury_holdings() {
    let creator = AccountId::new([9; 32]);
    let treasury = AccountId::new([10; 32]);
    let call = build_withdraw_factory_proceeds_invocation(sale(), creator, treasury, 0);
    let a = factory_pool_addresses(sale());
    let mentions = call.account_mentions();
    for account in [
        a.creator_escrow,
        associated_token_account(creator, sale().collateral_definition),
        associated_token_account(treasury, sale().collateral_definition),
    ] {
        let mention = mentions
            .iter()
            .find(|m| m.actor().account_id == account)
            .unwrap();
        assert!(!mention.authorizes);
        assert!(matches!(
            &mention.identity,
            wallet::AccountIdentity::PublicPda { .. }
        ));
    }
}

#[test]
fn allocation_claim_admits_the_creator_output_holding() {
    let creator = AccountId::new([9; 32]);
    let call = build_claim_creator_allocation_invocation(sale(), creator, 0);
    let holding =
        associated_token_account(creator, factory_pool_addresses(sale()).token_definition);
    assert!(
        call.account_mentions()
            .iter()
            .any(|m| m.actor().account_id == holding
                && !m.authorizes
                && matches!(m.identity, wallet::AccountIdentity::PublicPda { .. }))
    );
}

#[test]
fn creator_claim_uses_factory_escrow_and_derived_recipient_ata() {
    let creator = AccountId::new([9; 32]);
    let a = factory_pool_addresses(sale());
    let call = build_claim_creator_allocation_invocation(sale(), creator, 0);
    assert_eq!(call.destination, Actor::new(a.factory, FACTORY_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [creator]);
    assert!(call.actors.contains(&token_actor(associated_token_account(
        a.creator_escrow,
        a.token_definition
    ))));
    assert!(call.actors.contains(&token_actor(associated_token_account(
        creator,
        a.token_definition
    ))));
}

#[test]
fn saved_settlement_stages_identify_completed_manual_work() {
    for (stage, complete) in [
        (SettlementStage::Complete, true),
        (SettlementStage::Graduated, true),
        (SettlementStage::Unstarted, false),
        (SettlementStage::Ready, false),
        (
            SettlementStage::Withdrawn {
                unsold: 0,
                collateral_due: 0,
            },
            false,
        ),
        (
            SettlementStage::HookReady {
                unsold: 0,
                collateral_due: 0,
            },
            false,
        ),
    ] {
        let mut state = valid_factory_state(sale());
        state.settlement_stage = stage;
        assert_eq!(
            state.settlement_stage.is_manual_settlement_complete(),
            complete,
            "{stage:?}"
        );
    }
}

#[test]
fn settlement_builders_use_factory_actor_and_creator_signature() {
    let creator = AccountId::new([9; 32]);
    let claim = build_claim_creator_allocation_invocation(sale(), creator, 0);
    let withdrawal =
        build_withdraw_factory_proceeds_invocation(sale(), creator, AccountId::new([4; 32]), 0);
    assert_eq!(claim.destination, withdrawal.destination);
    assert_eq!(claim.signer_accounts, [creator]);
    assert_eq!(withdrawal.signer_accounts, [creator]);
}

#[test]
fn collection_is_unsigned_and_factory_withdrawal_removes_duplicate_treasury_ata() {
    let creator = AccountId::new([2; 32]);
    let pool = AccountId::new([4; 32]);
    let collection = build_collect_fees_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        pool,
        sale().collateral_definition,
        creator,
    );
    assert!(collection.signer_accounts.is_empty());
    assert_eq!(collection.destination, Actor::new(pool, CURVE_PROGRAM_ID));
    assert!(
        collection
            .actors
            .contains(&token_actor(associated_token_account(
                creator,
                sale().collateral_definition
            )))
    );
    let withdrawal = build_withdraw_factory_proceeds_invocation(sale(), creator, creator, 0);
    assert_eq!(
        withdrawal.actors.len(),
        withdrawal
            .actors
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
}

#[test]
fn collection_builder_omits_the_treasury_when_it_is_the_pool_vault() {
    let pool = AccountId::new([4; 32]);
    let call = build_collect_fees_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        pool,
        sale().collateral_definition,
        pool,
    );
    assert_eq!(
        call.actors.len(),
        call.actors
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
    assert!(call.signer_accounts.is_empty());
    assert_eq!(call.instruction, curve_core::Message::CollectFees);
}

#[test]
fn factory_withdrawal_routes_reserves_through_factory_policy() {
    let creator = AccountId::new([9; 32]);
    let a = factory_pool_addresses(sale());
    let call =
        build_withdraw_factory_proceeds_invocation(sale(), creator, AccountId::new([4; 32]), 0);
    assert_eq!(call.destination, Actor::new(a.factory, FACTORY_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [creator]);
    assert_eq!(
        call.instruction,
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 0 }
    );
}

#[test]
fn fee_targets_are_named() {
    let target = PoolFeeTarget {
        pool: AccountId::new([1; 32]),
        collateral_definition: AccountId::new([2; 32]),
    };
    assert_ne!(target.pool, target.collateral_definition);
}
