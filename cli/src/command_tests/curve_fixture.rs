use std::path::Path;

use factory_core::{CreationStage, FactoryState, GraduationPolicy, SettlementStage, TradeMode};
use launchpad_client::{BuyPolicy, EndingPolicy, FactorySale, PausePolicy};
use lee::AccountId;
use lee_core::account::Actor;

use super::{SALT, id, launchpad_transport::FixtureTransport};

pub(super) fn curve_write_args(command: &str, path: &Path, creator: AccountId) -> Vec<String> {
    let sale = curve_sale();
    let path = path.to_str().unwrap();
    [
        "launchpad",
        "--json",
        command,
        "--namespace",
        &sale.namespace.to_string(),
        "--launch-salt",
        &hex::encode(sale.launch_salt),
        "--creator",
        &creator.to_string(),
        "--collateral-definition",
        &sale.collateral_definition.to_string(),
        "--factory-program-path",
        path,
        "--factory-program-account",
        &sale.factory_program_id.to_string(),
        "--curve-program-path",
        path,
        "--curve-program-account",
        &sale.curve_program_id.to_string(),
    ]
    .map(str::to_owned)
    .into()
}

pub(super) fn curve_sale() -> FactorySale {
    FactorySale {
        namespace: id(1),
        factory_program_id: id(2),
        curve_program_id: id(3),
        launch_salt: SALT,
        collateral_definition: id(4),
    }
}

pub(super) fn curve_state(sale: FactorySale) -> FactoryState {
    let FactorySale {
        namespace,
        factory_program_id: factory_program,
        curve_program_id: curve_program,
        launch_salt,
        collateral_definition: collateral,
    } = sale;
    let factory = factory_core::compute_factory_pda(namespace, factory_program, launch_salt);
    let definition = factory_core::compute_definition_pda(namespace, factory_program, launch_salt);
    let bytes = borsh::to_vec(&(
        (
            BuyPolicy::Open,
            PausePolicy::Disabled,
            EndingPolicy::Depletion,
            TradeMode::BuyAndSell,
            0_u16,
            None::<u128>,
            None::<factory_core::GraduationHook>,
            CreationStage::Minted,
            SettlementStage::Unstarted,
            None::<u64>,
        ),
        (
            namespace,
            launch_salt,
            definition,
            collateral,
            800_u128,
            0_u128,
            GraduationPolicy::default(),
            50_u128,
            850_u128,
            2_000_u128,
            100_u128,
        ),
        (
            curve_program,
            factory_core::compute_curve_creator_role_pda(namespace, factory_program, launch_salt),
            "Fixture",
            "ipfs://fixture",
            factory_core::compute_escrow_pda(namespace, factory_program, launch_salt),
            curve_core::compute_pool_pda(namespace, curve_program, definition, collateral, factory),
            false,
            None::<u8>,
        ),
    ))
    .unwrap();
    FactoryState::decode(&bytes).unwrap()
}

pub(super) fn curve_transport() -> FixtureTransport {
    let sale = curve_sale();
    let factory = factory_core::compute_factory_pda(sale.namespace, sale.factory_program_id, SALT);
    let mut transport = FixtureTransport::default();
    transport.save(
        Actor::new(factory, sale.factory_program_id),
        borsh::to_vec(&curve_state(sale)).unwrap(),
    );
    transport
}

pub(super) fn funded_curve_pool(state: &FactoryState, factory: Actor) -> curve_core::PoolAccount {
    curve_core::PoolAccount {
        buy_policy: state.buy_policy.clone(),
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
        namespace: state.namespace,
        token0_definition_id: state.token_definition_id,
        token1_definition_id: state.collateral_definition_id,
        owner: factory.account_id,
        owner_program: Some((
            factory.program_account_id,
            *factory_core::compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
        )),
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: state.sale_reserve,
            token1_amount: 0,
            virtual_reserve0: state.virtual_token_reserve,
            virtual_reserve1: state.virtual_collateral_reserve,
            ending_policy: state.ending_policy,
            sell_fee_bps: state.sell_fee_bps,
            close_timestamp: state.end_timestamp,
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap(),
        block_allocation: Default::default(),
    }
}
