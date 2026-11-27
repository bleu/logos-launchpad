use factory_core::{CreationStage, FactoryState, SettlementStage};
use lee::AccountId;

use crate::curve::factory_pool_addresses;
use crate::lbp::lbp_addresses;
use crate::{CreateSaleRequest, FactorySale, GraduationPolicy, LbpSale, TradeMode};

pub(super) const FACTORY_PROGRAM_ID: AccountId = AccountId::new([7; 32]);
pub(super) const CURVE_PROGRAM_ID: AccountId = AccountId::new([6; 32]);

pub(super) fn sale() -> FactorySale {
    FactorySale {
        namespace: AccountId::new([0xAD; 32]),
        factory_program_id: FACTORY_PROGRAM_ID,
        curve_program_id: CURVE_PROGRAM_ID,
        launch_salt: [1; 32],
        collateral_definition: AccountId::new([5; 32]),
    }
}

pub(super) fn creation() -> CreateSaleRequest {
    CreateSaleRequest {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Depletion,
        trade_mode: factory_core::TradeMode::BuyAndSell,
        sell_fee_bps: 0,
        per_block_token_limit: None,
        graduation_hook: None,
        graduation_policy: GraduationPolicy::default(),
        name: "E2E token".into(),
        uri: "https://example.invalid/e2e-token.json".into(),
        graduation_reserve: 0,
        sale_reserve: 800,
        creator_allocation: 50,
        virtual_token_reserve: 2_000,
        virtual_collateral_reserve: 100,
        end_timestamp: None,
        now: 0,
    }
}

pub(super) fn pool_snapshot() -> (curve_core::PoolAccount, curve_core::Config) {
    let pool = curve_core::PoolAccount {
        buy_policy: curve_core::BuyPolicy::Open,
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
        block_allocation: Default::default(),
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: AccountId::new([2; 32]),
        token1_definition_id: AccountId::new([5; 32]),
        owner: AccountId::new([1; 32]),
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .unwrap(),
    };
    let config = curve_core::Config::decode(
        &borsh::to_vec(&(
            *b"LZNSCONF",
            3u16,
            pool.namespace,
            0u16,
            AccountId::new([4; 32]),
            AccountId::new([3; 32]),
            false,
        ))
        .unwrap(),
    )
    .unwrap();
    (pool, config)
}

pub(super) fn valid_factory_state(sale: FactorySale) -> FactoryState {
    let addresses = factory_pool_addresses(sale);
    let mut bytes = Vec::new();
    macro_rules! field {
        ($value:expr) => {
            borsh::BorshSerialize::serialize(&$value, &mut bytes).unwrap()
        };
    }
    field!(curve_core::BuyPolicy::Open);
    field!(curve_core::PausePolicy::Disabled);
    field!(curve_core::EndingPolicy::Depletion);
    field!(factory_core::TradeMode::BuyAndSell);
    field!(0u16);
    field!(None::<u128>);
    field!(None::<factory_core::GraduationHook>);
    field!(CreationStage::Active);
    field!(SettlementStage::Ready);
    field!(None::<u64>);
    field!(sale.namespace);
    field!(sale.launch_salt);
    field!(addresses.token_definition);
    field!(sale.collateral_definition);
    field!(800u128);
    field!(0u128); // Graduation reserve.
    field!(GraduationPolicy::default());
    field!(50u128);
    field!(850u128);
    field!(2_000u128);
    field!(100u128);
    field!(sale.curve_program_id);
    field!(factory_core::compute_curve_creator_role_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt
    ));
    field!("E2E token".to_owned());
    field!("https://example.invalid/e2e-token.json".to_owned());
    field!(addresses.creator_escrow);
    field!(addresses.pool);
    field!(false);
    field!(None::<u8>);
    FactoryState::decode(&bytes).unwrap()
}

pub(super) fn matching_factory_pool(state: &FactoryState) -> curve_core::PoolAccount {
    let (mut pool, _) = pool_snapshot();
    pool.namespace = state.namespace;
    pool.token0_definition_id = state.token_definition_id;
    pool.token1_definition_id = state.collateral_definition_id;
    pool.owner = factory_pool_addresses(sale()).factory;
    pool.owner_program = Some((
        sale().factory_program_id,
        *factory_core::compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
    ));
    pool.pool.virtual_reserve0 = state.virtual_token_reserve;
    pool.pool.virtual_reserve1 = state.virtual_collateral_reserve;
    pool.pool.initial_k = state.virtual_token_reserve * state.virtual_collateral_reserve;
    pool.pool.k = pool.pool.initial_k;
    pool.pool.sell_fee_bps = state.sell_fee_bps;
    pool.pool.ending_policy = state.ending_policy;
    pool.pool.close_timestamp = state.end_timestamp;
    pool.pool.close_on_depletion = match state.ending_policy {
        curve_core::EndingPolicy::Scheduled => None,
        _ => Some(pool::TokenSide::Token0),
    };
    pool
}

pub(super) fn lbp_sale() -> LbpSale {
    LbpSale {
        namespace: sale().namespace,
        namespace_program_id: sale().curve_program_id,
        factory_program_id: sale().factory_program_id,
        lbp_program_id: AccountId::new([12; 32]),
        launch_salt: sale().launch_salt,
        collateral_definition: sale().collateral_definition,
    }
}

pub(super) fn fixed_creation() -> factory_core::FixedPriceInputs {
    let sale = lbp_sale();
    let factory = factory_core::compute_factory_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt,
    );
    let definition = factory_core::compute_definition_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt,
    );
    let pool = lbp_core::compute_pool_pda(
        sale.namespace,
        sale.lbp_program_id,
        definition,
        sale.collateral_definition,
        factory,
    );
    factory_core::FixedPriceInputs {
        per_block_token_limit: None,
        buy_policy: curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: *sale.namespace.value(),
            sale_policy_id: *pool.value(),
            root: [42; 32],
        }),
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Either,
        trade_mode: TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: GraduationPolicy::default(),
        namespace: sale.namespace,
        launch_salt: sale.launch_salt,
        name: "Sale".into(),
        uri: "ipfs://sale".into(),
        graduation_reserve: 0,
        sale_inventory: 800,
        creator_allocation: 200,
        price: lbp_core::FixedPrice {
            collateral_units: 25,
            token_units: 10,
        },
        start: factory_core::SaleStart::Now { timestamp: 9 },
        end_timestamp: Some(100),
        allocation_release: factory_core::CreatorAllocationRelease::AfterSetup,
        namespace_program_id: sale.namespace_program_id,
        lbp_program_id: sale.lbp_program_id,
        collateral_definition_id: sale.collateral_definition,
        creator: AccountId::new([9; 32]),
        now: 9,
    }
}

pub(super) fn weighted_creation() -> factory_core::WeightedInputs {
    let fixed = fixed_creation();
    factory_core::WeightedInputs {
        per_block_token_limit: None,
        buy_policy: fixed.buy_policy,
        pause_policy: curve_core::PausePolicy::AuthorityControlled,
        ending_policy: curve_core::EndingPolicy::Scheduled,
        trade_mode: TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: GraduationPolicy::default(),
        namespace: fixed.namespace,
        launch_salt: fixed.launch_salt,
        name: fixed.name,
        uri: fixed.uri,
        graduation_reserve: 0,
        sale_inventory: fixed.sale_inventory,
        creator_allocation: fixed.creator_allocation,
        collateral_seed: 100,
        token_weight_start: lbp_core::WEIGHT_TOTAL / 2 + 1,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2 - 1,
        start_timestamp: 9,
        end_timestamp: 100,
        allocation_release: fixed.allocation_release,
        namespace_program_id: fixed.namespace_program_id,
        lbp_program_id: fixed.lbp_program_id,
        collateral_definition_id: fixed.collateral_definition_id,
        creator: fixed.creator,
    }
}

pub(super) fn valid_lbp_factory_state(weighted: bool) -> factory_core::LbpLaunchState {
    let sale = lbp_sale();
    let a = lbp_addresses(sale);
    let mut bytes = Vec::new();
    macro_rules! field {
        ($value:expr) => {
            bytes.extend(borsh::to_vec(&$value).unwrap());
        };
    }
    field!(None::<u128>);
    field!(curve_core::BuyPolicy::Open);
    field!(curve_core::PausePolicy::Disabled);
    field!(if weighted {
        curve_core::EndingPolicy::Scheduled
    } else {
        curve_core::EndingPolicy::Either
    });
    field!(TradeMode::BuyOnly);
    field!(0u16);
    field!(Some(factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![7, 11]
    }));
    field!(GraduationPolicy::default());
    field!(if weighted { *b"LZWGLAUN" } else { *b"LZFXLAUN" });
    field!(8u16);
    field!(factory_core::compute_creator_role_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt
    ));
    field!(if weighted {
        factory_core::LbpSetupMode::Weighted {
            collateral_seed: 100,
            token_weight_start: lbp_core::WEIGHT_TOTAL / 2 + 1,
            token_weight_end: lbp_core::WEIGHT_TOTAL / 2 - 1,
        }
    } else {
        factory_core::LbpSetupMode::FixedPrice
    });
    field!(factory_core::LbpCreationStage::Complete);
    field!(lbp_core::SettlementProgress::Unselected);
    field!(sale.namespace);
    field!(sale.launch_salt);
    field!(a.token_definition);
    field!(sale.collateral_definition);
    field!(a.creator_escrow);
    field!(a.pool);
    field!(sale.namespace_program_id);
    field!(sale.lbp_program_id);
    field!(800u128);
    field!(0u128); // Graduation reserve.
    field!(200u128);
    field!(1_000u128);
    field!(if weighted {
        lbp_core::FixedPrice {
            collateral_units: 0,
            token_units: 0,
        }
    } else {
        lbp_core::FixedPrice {
            collateral_units: 25,
            token_units: 10,
        }
    });
    field!(9u64);
    field!(Some(100u64));
    field!(factory_core::CreatorAllocationRelease::AfterEnd);
    field!(false);
    field!("Sale".to_owned());
    field!("ipfs://sale".to_owned());
    field!(None::<u8>);
    field!(None::<u8>);
    factory_core::LbpLaunchState::decode(&bytes).unwrap()
}
