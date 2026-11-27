use common::{HashType, block::TransactionReceipt};
use lee::AccountId;
use lee_core::account::Actor;

use crate::lbp::lbp_addresses;
use crate::{
    ClientError, LbpCreateRequest, LbpSale, ProgramHandle, SaleInfo, SaleSelection, WriteTransport,
    create_lbp_sale, lbp_buy_policy, load_sale_info,
};

use super::fixtures::{fixed_creation, lbp_sale, valid_lbp_factory_state, weighted_creation};
use super::launchpad_transport::{FixtureTransport, creator_role_state, write_wallet};
use super::lbp_fixture;

const HASH: HashType = HashType([51; 32]);

fn factory_actor(sale: LbpSale) -> Actor {
    Actor::new(lbp_addresses(sale).factory, sale.factory_program_id)
}

#[tokio::test]
async fn fixed_price_creation_verifies_funding_and_the_saved_start() {
    use factory_core::{CreatorAllocationRelease, SaleStart};

    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    for (start, saved_start, release, released) in [
        (
            SaleStart::Now { timestamp: 0 },
            0,
            CreatorAllocationRelease::AfterSetup,
            true,
        ),
        (
            SaleStart::At { timestamp: 17 },
            17,
            CreatorAllocationRelease::AfterEnd,
            false,
        ),
    ] {
        let mut transport = FixtureTransport::default();
        transport.deploy(factory.account_id, factory.image_id());
        transport.acknowledge(HASH, Some(TransactionReceipt::EXEMPT));
        let home = tempfile::tempdir().unwrap();
        let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
        let mut inputs = fixed_creation();
        inputs.creator = signer;
        inputs.start = start;
        inputs.allocation_release = release;
        let mut state = valid_lbp_factory_state(false);
        state.start_timestamp = saved_start;
        state.buy_policy = inputs.buy_policy.clone();
        state.graduation_hook = None;
        state.allocation_release = release;
        state.creator_allocation_released = released;
        let actor = factory_actor(sale);
        transport.save_after_receipt(HASH, actor, borsh::to_vec(&state).unwrap());
        transport.save_after_receipt(
            HASH,
            Actor::new(state.pool_id, sale.lbp_program_id),
            borsh::to_vec(&lbp_fixture::funded_pool(actor, &state)).unwrap(),
        );

        let receipt = create_lbp_sale(&wallet, &factory, LbpCreateRequest::FixedPrice(inputs))
            .await
            .unwrap();
        assert_eq!(receipt.transaction_hashes, [HASH]);
        assert_eq!(receipt.authority_holders, [signer]);
        assert_eq!(transport.sent_count(), 1);
        let SaleInfo::FixedPrice(info) =
            load_sale_info(&transport, SaleSelection::FixedPrice(sale))
                .await
                .unwrap()
        else {
            panic!("expected fixed-price terms");
        };
        assert_eq!(info.sale.start_timestamp, saved_start);
        assert_eq!(info.sale.allocation_release, release);
        assert_eq!(info.sale.creator_allocation_released, released);
        assert!(info.sale.pool.unwrap().funded);
    }
}
#[tokio::test]
async fn lbp_creation_does_not_accept_an_unpaid_after_setup_allocation() {
    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    let mut transport = FixtureTransport::default();
    transport.deploy(factory.account_id, factory.image_id());
    transport.acknowledge(HASH, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let mut inputs = fixed_creation();
    inputs.creator = signer;
    let mut state = valid_lbp_factory_state(false);
    state.buy_policy = inputs.buy_policy.clone();
    state.graduation_hook = None;
    state.allocation_release = factory_core::CreatorAllocationRelease::AfterSetup;
    state.creator_allocation_released = false;
    let actor = factory_actor(sale);
    transport.save_after_receipt(HASH, actor, borsh::to_vec(&state).unwrap());
    transport.save_after_receipt(
        HASH,
        Actor::new(state.pool_id, sale.lbp_program_id),
        borsh::to_vec(&lbp_fixture::funded_pool(actor, &state)).unwrap(),
    );

    let result = create_lbp_sale(&wallet, &factory, LbpCreateRequest::FixedPrice(inputs)).await;
    assert!(
        matches!(
            result,
            Err(ClientError::CreationVerificationFailed { hash, ref source }) if hash == HASH
                && matches!(source.as_ref(), ClientError::CreationIncomplete)
        ),
        "{result:?}"
    );
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn a_funded_lbp_sale_accepts_only_an_authorized_matching_repeat() {
    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    let mut transport = FixtureTransport::default();
    let key = lee::PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&lee::PublicKey::new_from_private_key(&key));
    let mut inputs = fixed_creation();
    inputs.creator = signer;
    let mut state = valid_lbp_factory_state(false);
    state.buy_policy = inputs.buy_policy.clone();
    state.graduation_hook = None;
    state.allocation_release = inputs.allocation_release;
    state.creator_allocation_released = true;
    let actor = factory_actor(sale);
    let role = Actor::new(
        state.creator_role_id,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    );
    transport.save(actor, borsh::to_vec(&state).unwrap());
    transport.save(
        Actor::new(state.pool_id, sale.lbp_program_id),
        borsh::to_vec(&lbp_fixture::funded_pool(actor, &state)).unwrap(),
    );
    transport.save(role, creator_role_state(role, actor, signer));
    let home = tempfile::tempdir().unwrap();
    let (wallet, _) = write_wallet(home.path(), transport.clone(), WriteTransport::new);

    let receipt = create_lbp_sale(
        &wallet,
        &factory,
        LbpCreateRequest::FixedPrice(inputs.clone()),
    )
    .await
    .unwrap();
    assert!(receipt.transaction_hashes.is_empty());

    let mut conflict = inputs.clone();
    conflict.buy_policy = curve_core::BuyPolicy::Open;
    assert!(matches!(
        create_lbp_sale(&wallet, &factory, LbpCreateRequest::FixedPrice(conflict)).await,
        Err(ClientError::LaunchParametersMismatch)
    ));

    let holder = AccountId::new([45; 32]);
    transport.save(role, creator_role_state(role, actor, holder));
    let home = tempfile::tempdir().unwrap();
    let (wallet, _) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    assert!(matches!(
        create_lbp_sale(
            &wallet,
            &factory,
            LbpCreateRequest::FixedPrice(inputs.clone())
        )
        .await,
        Err(ClientError::CreatorAuthorityMismatch)
    ));
    inputs.creator = holder;
    assert!(matches!(
        create_lbp_sale(&wallet, &factory, LbpCreateRequest::FixedPrice(inputs)).await,
        Err(ClientError::PublicSigningKeyUnavailable(id)) if id == holder
    ));
    assert_eq!(transport.sent_count(), 0);
}

#[tokio::test(start_paused = true)]
async fn lbp_creation_keeps_the_hash_until_funded_setup_is_verified() {
    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    let reverted = TransactionReceipt {
        success: false,
        ..TransactionReceipt::EXEMPT
    };
    for (weighted, selection) in [
        (false, SaleSelection::FixedPrice(sale)),
        (true, SaleSelection::Weighted(sale)),
    ] {
        for receipt in [None, Some(reverted), Some(TransactionReceipt::EXEMPT)] {
            let mut transport = FixtureTransport::default();
            transport.deploy(factory.account_id, factory.image_id());
            transport.acknowledge(HASH, receipt);
            let mut state = valid_lbp_factory_state(weighted);
            state.creation_stage = factory_core::LbpCreationStage::Allocated;
            let actor = factory_actor(sale);
            transport.save_after_receipt(HASH, actor, borsh::to_vec(&state).unwrap());
            let home = tempfile::tempdir().unwrap();
            let (wallet, signer) =
                write_wallet(home.path(), transport.clone(), WriteTransport::new);
            let request = if weighted {
                let mut inputs = weighted_creation();
                inputs.creator = signer;
                LbpCreateRequest::Weighted(inputs)
            } else {
                let mut inputs = fixed_creation();
                inputs.creator = signer;
                LbpCreateRequest::FixedPrice(inputs)
            };

            let result = create_lbp_sale(&wallet, &factory, request).await;
            let actual = load_sale_info(&transport, selection).await;
            match receipt {
                None => assert!(
                    matches!(result, Err(ClientError::PublicWritePending { hash: Some(hash), .. }) if hash == HASH),
                    "{selection:?}: {result:?}"
                ),
                Some(receipt) if !receipt.success => assert!(
                    matches!(result, Err(ClientError::PublicWriteReverted { hash }) if hash == HASH),
                    "{selection:?}: {result:?}"
                ),
                Some(_) => assert!(
                    matches!(result, Err(ClientError::CreationVerificationFailed { hash, ref source })
                        if hash == HASH && matches!(**source, ClientError::CreationIncomplete)),
                    "{selection:?}: {result:?}"
                ),
            }
            if receipt.is_some_and(|receipt| receipt.success) {
                let sale = match actual.unwrap() {
                    SaleInfo::FixedPrice(info) => info.sale,
                    SaleInfo::Weighted(info) => info.sale,
                    SaleInfo::BondingCurve(_) => {
                        panic!("expected fixed-price or weighted terms")
                    }
                };
                assert_eq!(sale.setup_stage, factory_core::LbpCreationStage::Allocated);
                assert!(sale.pool.is_none());
            } else {
                assert!(matches!(actual, Err(ClientError::FactoryLaunchMissing)));
            }
            assert_eq!(transport.sent_count(), 1);
        }
    }
}

#[tokio::test]
async fn lbp_creation_rejects_wrong_bound_accounts_without_a_write() {
    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    let actor = factory_actor(sale);
    let selection = SaleSelection::Weighted(sale);
    let foreign = AccountId::new([45; 32]);
    for wrong_role in [true, false] {
        let mut state = valid_lbp_factory_state(true);
        let mut pool = lbp_fixture::funded_pool(actor, &state);
        if wrong_role {
            state.creator_role_id = foreign;
        } else {
            pool.factory_owner = foreign;
        }
        let mut transport = FixtureTransport::default();
        transport.save(actor, borsh::to_vec(&state).unwrap());
        transport.save(
            Actor::new(state.pool_id, sale.lbp_program_id),
            borsh::to_vec(&pool).unwrap(),
        );
        let home = tempfile::tempdir().unwrap();
        let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
        let mut inputs = weighted_creation();
        inputs.creator = signer;

        assert!(matches!(
            load_sale_info(&transport, selection).await,
            Err(ClientError::FactorySaleMismatch)
        ));
        assert!(matches!(
            create_lbp_sale(&wallet, &factory, LbpCreateRequest::Weighted(inputs)).await,
            Err(ClientError::FactorySaleMismatch)
        ));
        assert_eq!(transport.sent_count(), 0);
    }
}

#[tokio::test]
async fn invalid_lbp_creation_terms_do_not_submit_a_transaction() {
    let factory = ProgramHandle {
        account_id: lbp_sale().factory_program_id,
        image: programs::clock(),
    };
    let transport = FixtureTransport::default();
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let mut fixed = fixed_creation();
    fixed.creator = signer;
    fixed.price.token_units = 0;
    let mut weighted = weighted_creation();
    weighted.creator = signer;
    weighted.end_timestamp = 8;
    for request in [
        LbpCreateRequest::FixedPrice(fixed),
        LbpCreateRequest::Weighted(weighted),
    ] {
        assert!(matches!(
            create_lbp_sale(&wallet, &factory, request).await,
            Err(ClientError::FactoryPolicy(
                factory_core::Error::InvalidTerms
            ))
        ));
    }
    assert_eq!(transport.sent_count(), 0);
}

#[tokio::test]
async fn lbp_creation_rejects_an_allowlist_bound_to_another_pool() {
    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    let mut transport = FixtureTransport::default();
    transport.deploy(factory.account_id, factory.image_id());
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let mut other_sale = sale;
    other_sale.launch_salt = [45; 32];
    let foreign_policy = lbp_buy_policy(other_sale, Some([42; 32]));
    let mut fixed = fixed_creation();
    fixed.creator = signer;
    fixed.buy_policy = foreign_policy.clone();
    let mut weighted = weighted_creation();
    weighted.creator = signer;
    weighted.buy_policy = foreign_policy;

    for request in [
        LbpCreateRequest::FixedPrice(fixed),
        LbpCreateRequest::Weighted(weighted),
    ] {
        transport.acknowledge(HASH, Some(TransactionReceipt::EXEMPT));
        let result = create_lbp_sale(&wallet, &factory, request).await;
        assert!(
            matches!(
                result,
                Err(ClientError::FactoryPolicy(
                    factory_core::Error::InvalidTerms
                ))
            ),
            "{result:?}"
        );
    }
    assert_eq!(transport.sent_count(), 0);
}

#[tokio::test]
async fn weighted_creation_preserves_the_explicit_schedule_and_seed() {
    let sale = lbp_sale();
    let factory = ProgramHandle {
        account_id: sale.factory_program_id,
        image: programs::clock(),
    };
    let mut transport = FixtureTransport::default();
    transport.deploy(factory.account_id, factory.image_id());
    transport.acknowledge(HASH, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let mut inputs = weighted_creation();
    inputs.creator = signer;
    let mut state = valid_lbp_factory_state(true);
    state.buy_policy = inputs.buy_policy.clone();
    state.pause_policy = inputs.pause_policy;
    state.graduation_hook = None;
    state.allocation_release = inputs.allocation_release;
    state.creator_allocation_released = true;
    let actor = factory_actor(sale);
    transport.save_after_receipt(HASH, actor, borsh::to_vec(&state).unwrap());
    transport.save_after_receipt(
        HASH,
        Actor::new(state.pool_id, sale.lbp_program_id),
        borsh::to_vec(&lbp_fixture::funded_pool(actor, &state)).unwrap(),
    );

    let receipt = create_lbp_sale(&wallet, &factory, LbpCreateRequest::Weighted(inputs))
        .await
        .unwrap();
    assert_eq!(receipt.transaction_hashes, [HASH]);
    assert_eq!(transport.sent_count(), 1);
    let SaleInfo::Weighted(info) = load_sale_info(&transport, SaleSelection::Weighted(sale))
        .await
        .unwrap()
    else {
        panic!("expected weighted terms");
    };
    assert_eq!(info.collateral_seed, 100);
    assert_eq!(info.sale.start_timestamp, 9);
    assert_eq!(info.sale.terms.end_timestamp, Some(100));
    assert!(info.sale.pool.unwrap().funded);
}
