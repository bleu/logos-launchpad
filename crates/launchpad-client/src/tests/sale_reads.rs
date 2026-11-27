use factory_core::{CreationStage, SettlementStage};
use lee_core::account::Actor;

use crate::curve::factory_pool_addresses;
use crate::lbp::lbp_addresses;
use crate::{ClientError, SaleInfo, SaleSelection, load_sale_info};

use super::fixtures::{
    lbp_sale, matching_factory_pool, sale, valid_factory_state, valid_lbp_factory_state,
};
use super::launchpad_transport;

#[tokio::test]
async fn reads_saved_curve_terms_before_the_pool_exists() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.creation_stage = CreationStage::Minted;
    state.settlement_stage = SettlementStage::Unstarted;
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.save(
        Actor::new(
            factory_pool_addresses(sale).factory,
            sale.factory_program_id,
        ),
        borsh::to_vec(&state).unwrap(),
    );

    let SaleInfo::BondingCurve(info) =
        load_sale_info(&transport, SaleSelection::BondingCurve(sale))
            .await
            .unwrap()
    else {
        panic!("expected bonding-curve terms");
    };
    assert_eq!(info.setup_stage, CreationStage::Minted);
    assert_eq!(info.terms.sale_inventory, 800);
    assert_eq!(info.initial_virtual_collateral_reserve, 100);
    assert!(info.pool.is_none());
}

fn saved_lbp_transport(weighted: bool) -> launchpad_transport::FixtureTransport {
    let sale = lbp_sale();
    let mut state = valid_lbp_factory_state(weighted);
    state.creation_stage = factory_core::LbpCreationStage::Allocated;
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.save(
        Actor::new(lbp_addresses(sale).factory, sale.factory_program_id),
        borsh::to_vec(&state).unwrap(),
    );
    transport
}

#[tokio::test]
async fn reads_exact_fixed_price_before_pool_setup() {
    let SaleInfo::FixedPrice(info) = load_sale_info(
        &saved_lbp_transport(false),
        SaleSelection::FixedPrice(lbp_sale()),
    )
    .await
    .unwrap() else {
        panic!("expected fixed-price terms");
    };
    assert_eq!(
        info.sale.setup_stage,
        factory_core::LbpCreationStage::Allocated
    );
    assert_eq!(info.sale.terms.creator_allocation, 200);
    assert_eq!(info.price.collateral_units, 25);
    assert_eq!(info.price.token_units, 10);
}

#[tokio::test]
async fn reads_saved_weighted_seed_and_schedule() {
    let SaleInfo::Weighted(info) = load_sale_info(
        &saved_lbp_transport(true),
        SaleSelection::Weighted(lbp_sale()),
    )
    .await
    .unwrap() else {
        panic!("expected weighted terms");
    };
    assert_eq!(
        info.sale.setup_stage,
        factory_core::LbpCreationStage::Allocated
    );
    assert_eq!(info.sale.start_timestamp, 9);
    assert_eq!(info.sale.terms.end_timestamp, Some(100));
    assert_eq!(info.collateral_seed, 100);
    assert_eq!(info.token_weight_start, 500_000_000_000_000_001);
    assert_eq!(info.token_weight_end, 499_999_999_999_999_999);
}

#[tokio::test]
async fn a_shared_factory_address_does_not_identify_the_pricing_engine() {
    let transport = saved_lbp_transport(false);
    for selection in [
        SaleSelection::BondingCurve(sale()),
        SaleSelection::Weighted(lbp_sale()),
    ] {
        assert!(matches!(
            load_sale_info(&transport, selection).await,
            Err(ClientError::SaleEngineMismatch)
        ));
    }
}

#[tokio::test]
async fn active_curve_read_keeps_saved_and_observed_reserves_separate() {
    let sale = sale();
    let state = valid_factory_state(sale);
    let mut pool = matching_factory_pool(&state);
    pool.pool.real_reserve0 = 700;
    pool.pool.virtual_reserve1 = 120;
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.save(
        Actor::new(
            factory_pool_addresses(sale).factory,
            sale.factory_program_id,
        ),
        borsh::to_vec(&state).unwrap(),
    );
    transport.save(
        Actor::new(state.pool_id, sale.curve_program_id),
        borsh::to_vec(&pool).unwrap(),
    );

    let SaleInfo::BondingCurve(info) =
        load_sale_info(&transport, SaleSelection::BondingCurve(sale))
            .await
            .unwrap()
    else {
        panic!("expected bonding-curve terms");
    };
    assert_eq!(info.initial_virtual_collateral_reserve, 100);
    let pool = info.pool.unwrap();
    assert_eq!(pool.state.virtual_reserve1, 120);
    assert!(pool.funded);
    assert_eq!(pool.tokens_sold, Some(100));
    assert_eq!(pool.state.lifecycle, pool::PoolLifecycle::Open);
}

#[tokio::test]
async fn sale_read_distinguishes_missing_actor_state_from_malformed_bytes() {
    let sale = sale();
    let factory = factory_pool_addresses(sale).factory;
    let selection = SaleSelection::BondingCurve(sale);
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.save(Actor::new(factory, sale.curve_program_id), vec![1]);
    assert!(matches!(
        load_sale_info(&transport, selection).await,
        Err(ClientError::FactoryLaunchMissing)
    ));
    transport.save(Actor::new(factory, sale.factory_program_id), vec![1]);
    assert!(matches!(
        load_sale_info(&transport, selection).await,
        Err(ClientError::DecodeFactoryState(_))
    ));
}
