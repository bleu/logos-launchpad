use common::{HashType, block::TransactionReceipt};
use factory_core::{CreationStage, FactoryState, SettlementStage};
use lee::AccountId;
use lee_core::account::Actor;

use crate::curve::factory_pool_addresses;
use crate::{
    AccountLocation, ClientError, FactorySale, FactorySession, ProgramHandle, SaleInfo,
    SaleSelection, WriteTransport, curve_buy_policy, load_sale_info,
};

use super::fixtures::{creation, matching_factory_pool, sale, valid_factory_state};
use super::launchpad_transport::{FixtureTransport, creator_role_state, write_wallet};

const HASH: HashType = HashType([33; 32]);

fn programs(sale: FactorySale) -> (ProgramHandle, ProgramHandle) {
    (
        ProgramHandle {
            account_id: sale.factory_program_id,
            image: programs::clock(),
        },
        ProgramHandle {
            account_id: sale.curve_program_id,
            image: programs::clock(),
        },
    )
}

fn factory_actor(sale: FactorySale) -> Actor {
    Actor::new(
        factory_pool_addresses(sale).factory,
        sale.factory_program_id,
    )
}

fn role_actor(state: &FactoryState) -> Actor {
    Actor::new(
        state.creator_role_id,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    )
}

#[tokio::test]
async fn sends_one_creation_then_accepts_only_a_matching_repeat() {
    let sale = sale();
    let (factory, curve) = programs(sale);
    let mut transport = FixtureTransport::default();
    transport.deploy(factory.account_id, factory.image_id());
    transport.acknowledge(HASH, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let session = FactorySession::new(
        &wallet,
        &factory,
        &curve,
        None,
        sale.namespace,
        AccountLocation::Public(signer),
    );
    let mut request = creation();
    request.buy_policy = curve_buy_policy(sale, Some([42; 32]));
    let mut state = valid_factory_state(sale);
    state.buy_policy = request.buy_policy.clone();
    let mut pool = matching_factory_pool(&state);
    pool.buy_policy = state.buy_policy.clone();
    let factory_actor = factory_actor(sale);
    let role = role_actor(&state);
    transport.save_after_receipt(HASH, factory_actor, borsh::to_vec(&state).unwrap());
    transport.save_after_receipt(
        HASH,
        Actor::new(state.pool_id, sale.curve_program_id),
        borsh::to_vec(&pool).unwrap(),
    );
    transport.save_after_receipt(HASH, role, creator_role_state(role, factory_actor, signer));

    let receipt = session.create_sale(sale, request.clone()).await.unwrap();
    assert_eq!(receipt.transaction_hashes, [HASH]);
    assert_eq!(receipt.authority_holders, [signer]);
    let SaleInfo::BondingCurve(info) =
        load_sale_info(&transport, SaleSelection::BondingCurve(sale))
            .await
            .unwrap()
    else {
        panic!("expected bonding-curve terms");
    };
    assert_eq!(info.setup_stage, CreationStage::Active);
    assert!(info.pool.unwrap().funded);
    assert_eq!(info.terms.buy_policy, request.buy_policy);

    let repeat = session.create_sale(sale, request.clone()).await.unwrap();
    assert!(repeat.transaction_hashes.is_empty());
    assert!(repeat.authority_holders.is_empty());
    for root in [None, Some([43; 32])] {
        request.buy_policy = curve_buy_policy(sale, root);
        assert!(matches!(
            session.create_sale(sale, request.clone()).await,
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn matching_repeat_requires_the_current_creator_and_their_key() {
    let sale = sale();
    let (factory, curve) = programs(sale);
    let state = valid_factory_state(sale);
    let mut transport = FixtureTransport::default();
    let factory_actor = factory_actor(sale);
    transport.save(factory_actor, borsh::to_vec(&state).unwrap());
    transport.save(
        Actor::new(state.pool_id, sale.curve_program_id),
        borsh::to_vec(&matching_factory_pool(&state)).unwrap(),
    );
    let holder = AccountId::new([45; 32]);
    let role = role_actor(&state);
    transport.save(role, creator_role_state(role, factory_actor, holder));
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let create_as = |selected| {
        FactorySession::new(
            &wallet,
            &factory,
            &curve,
            None,
            sale.namespace,
            AccountLocation::Public(selected),
        )
    };
    assert!(matches!(
        create_as(signer).create_sale(sale, creation()).await,
        Err(ClientError::CreatorAuthorityMismatch)
    ));
    assert!(matches!(
        create_as(holder).create_sale(sale, creation()).await,
        Err(ClientError::PublicSigningKeyUnavailable(id)) if id == holder
    ));
    assert_eq!(transport.sent_count(), 0);
}

#[tokio::test]
async fn incomplete_or_noncanonical_setup_cannot_be_replayed() {
    let sale = sale();
    let (factory, curve) = programs(sale);
    for (stage, canonical) in [
        (CreationStage::Minted, true),
        (CreationStage::Active, true),
        (CreationStage::Active, false),
    ] {
        let mut state = valid_factory_state(sale);
        state.creation_stage = stage;
        state.settlement_stage = SettlementStage::Unstarted;
        let mut pool = matching_factory_pool(&state);
        pool.funded = false;
        if !canonical {
            pool.owner = AccountId::new([46; 32]);
        }
        let mut transport = FixtureTransport::default();
        transport.save(factory_actor(sale), borsh::to_vec(&state).unwrap());
        transport.save(
            Actor::new(state.pool_id, sale.curve_program_id),
            borsh::to_vec(&pool).unwrap(),
        );
        let home = tempfile::tempdir().unwrap();
        let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
        let session = FactorySession::new(
            &wallet,
            &factory,
            &curve,
            None,
            sale.namespace,
            AccountLocation::Public(signer),
        );
        if stage == CreationStage::Active && canonical {
            let SaleInfo::BondingCurve(info) =
                load_sale_info(&transport, SaleSelection::BondingCurve(sale))
                    .await
                    .unwrap()
            else {
                panic!("expected bonding-curve terms");
            };
            assert!(!info.pool.unwrap().funded);
        }
        let result = session.create_sale(sale, creation()).await;
        if canonical {
            assert!(
                matches!(result, Err(ClientError::CreationIncomplete)),
                "{result:?}"
            );
        } else {
            assert!(
                matches!(result, Err(ClientError::FactorySaleMismatch)),
                "{result:?}"
            );
        }
        assert_eq!(transport.sent_count(), 0);
    }
}

#[tokio::test(start_paused = true)]
async fn creation_preserves_the_hash_without_claiming_funded_setup() {
    let sale = sale();
    let (factory, curve) = programs(sale);
    let reverted = TransactionReceipt {
        success: false,
        ..TransactionReceipt::EXEMPT
    };
    for receipt in [None, Some(reverted), Some(TransactionReceipt::EXEMPT)] {
        let mut transport = FixtureTransport::default();
        if receipt.is_none() {
            // Model an underfunded send that is acknowledged but gets no receipt.
            transport.set_signer_balance(0);
        }
        transport.deploy(factory.account_id, factory.image_id());
        transport.acknowledge(HASH, receipt);
        let home = tempfile::tempdir().unwrap();
        let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
        let session = FactorySession::new(
            &wallet,
            &factory,
            &curve,
            None,
            sale.namespace,
            AccountLocation::Public(signer),
        );
        let result = session.create_sale(sale, creation()).await;
        match receipt {
            None => assert!(
                matches!(
                    result,
                    Err(ClientError::PublicWritePending { hash: Some(hash), .. }) if hash == HASH
                ),
                "{result:?}"
            ),
            Some(receipt) if !receipt.success => assert!(
                matches!(
                    result,
                    Err(ClientError::PublicWriteReverted { hash }) if hash == HASH
                ),
                "{result:?}"
            ),
            Some(_) => assert!(
                matches!(
                    result,
                    Err(ClientError::CreationVerificationFailed { hash, ref source })
                        if hash == HASH && matches!(**source, ClientError::CreationIncomplete)
                ),
                "{result:?}"
            ),
        }
        assert_eq!(transport.sent_count(), 1);
    }
}
