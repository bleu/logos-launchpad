use super::*;
use factory_core::{CreationStage, GraduationPolicy, LbpCreationStage, LbpLaunchState};
use launchpad_client::{
    CreateSaleRequest, FactorySale, PublicInvocation, build_create_fixed_price_invocation,
    build_create_sale_invocation, build_create_weighted_invocation,
};

const LBP: AccountId = AccountId::new([14; 32]);
const CYCLE_BUDGET: u64 = 10_000_000;

fn execute_create<I: borsh::BorshSerialize>(
    state: &mut V03State,
    signer: &PrivateKey,
    call: PublicInvocation<I>,
    timestamp_ms: u64,
) -> u64 {
    let creator = id(signer);
    assert_eq!(call.signer_accounts, [creator]);
    let mentions = call.account_mentions();
    let evidence = mentions
        .iter()
        .filter(|mention| {
            state
                .get_account_by_id_ref(mention.actor().account_id)
                .is_none()
        })
        .filter_map(|mention| match mention.identity {
            wallet::AccountIdentity::PublicPda { program, seed } => {
                Some(lee::PublicAccountEvidence::Pda { program, seed })
            }
            _ => None,
        })
        .collect();
    let nonce = state.get_account_by_id(creator).nonce;
    let message = Message::new(
        call.destination,
        call.message_bytes().unwrap(),
        mentions.iter().map(|mention| mention.actor()),
        BTreeMap::from([(creator, nonce)]),
        Some(lee::FeeDeclaration::new(
            creator,
            CYCLE_BUDGET,
            0,
            100_000_000,
        )),
        evidence,
    );
    let witnesses = WitnessSet::for_message(&message, &[signer]);
    let transaction = PublicTransaction::new(message, witnesses);
    let (charge, diff) = lee::ValidatedStateDiff::from_public_transaction_metered(
        &transaction,
        state,
        1,
        timestamp_ms,
        CYCLE_BUDGET,
    );
    assert!(
        charge.revert.is_none(),
        "create reverted: {:?}",
        charge.revert
    );
    state.apply_state_diff(diff.unwrap()).unwrap();
    assert_eq!(state.get_account_by_id(creator).nonce.0, nonce.0 + 1);
    charge.cycles
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn sdk_curve_create_funds_the_canonical_sale() {
    let mut fixture = factory_fixture();
    let signer = key(48);
    let creator = id(&signer);
    let sale = FactorySale {
        namespace: fixture.namespace,
        factory_program_id: FACTORY,
        curve_program_id: CURVE,
        launch_salt: SALT,
        collateral_definition: COLLATERAL,
    };
    let request = CreateSaleRequest {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Either,
        trade_mode: factory_core::TradeMode::BuyAndSell,
        sell_fee_bps: 777,
        per_block_token_limit: None,
        graduation_hook: None,
        name: "Curve".into(),
        uri: "https://example.test/curve".into(),
        sale_reserve: 800,
        graduation_reserve: 100,
        graduation_policy: GraduationPolicy::default(),
        creator_allocation: 200,
        virtual_token_reserve: 2_000,
        virtual_collateral_reserve: 100,
        end_timestamp: Some(4),
        now: 1,
    };
    let call = build_create_sale_invocation(sale, creator, request.clone()).unwrap();
    let cycles = execute_create(
        &mut fixture.state,
        &signer,
        call,
        fixture.inclusion_timestamp_ms,
    );
    let state = &fixture.state;
    let launch = factory_core::FactoryState::decode(
        state
            .get_account_by_id(fixture.factory)
            .data
            .actor_state(FACTORY),
    )
    .unwrap();
    assert_eq!(launch.creation_stage, CreationStage::Active);
    let pool = curve_core::PoolAccount::decode(
        state
            .get_account_by_id(fixture.pool)
            .data
            .actor_state(CURVE),
    )
    .unwrap();
    assert!(pool.funded);
    assert_eq!(balance(state, fixture.pool, fixture.definition), 800);
    assert!(
        launchpad_client::next_creation_invocation(sale, creator, request, Some(&launch))
            .unwrap()
            .is_none()
    );
    eprintln!("SDK public curve create: {cycles} cycles");
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn sdk_lbp_creates_fund_inventory_and_weighted_seed() {
    for weighted in [false, true] {
        let fixture = factory_fixture();
        let mut state = fixture
            .state
            .with_programs_at([(LBP, guest("LAUNCHPAD_LBP_ELF"), true)]);
        let signer = key(48);
        let creator = id(&signer);
        let call = if weighted {
            build_create_weighted_invocation(
                FACTORY,
                factory_core::WeightedInputs {
                    per_block_token_limit: None,
                    buy_policy: curve_core::BuyPolicy::Open,
                    pause_policy: curve_core::PausePolicy::Disabled,
                    ending_policy: lbp_core::EndingPolicy::Scheduled,
                    trade_mode: lbp_core::TradeMode::BuyOnly,
                    sell_fee_bps: 0,
                    graduation_hook: None,
                    graduation_policy: GraduationPolicy::default(),
                    namespace: fixture.namespace,
                    launch_salt: SALT,
                    name: "Weighted".into(),
                    uri: "https://example.test/weighted".into(),
                    sale_inventory: 800,
                    graduation_reserve: 100,
                    creator_allocation: 200,
                    collateral_seed: 100,
                    token_weight_start: lbp_core::WEIGHT_TOTAL * 4 / 5,
                    token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                    start_timestamp: 2,
                    end_timestamp: 4,
                    allocation_release: factory_core::CreatorAllocationRelease::AfterSetup,
                    namespace_program_id: CURVE,
                    lbp_program_id: LBP,
                    collateral_definition_id: COLLATERAL,
                    creator,
                },
            )
        } else {
            build_create_fixed_price_invocation(
                FACTORY,
                factory_core::FixedPriceInputs {
                    per_block_token_limit: None,
                    buy_policy: curve_core::BuyPolicy::Open,
                    pause_policy: curve_core::PausePolicy::Disabled,
                    ending_policy: lbp_core::EndingPolicy::Either,
                    trade_mode: lbp_core::TradeMode::BuyOnly,
                    sell_fee_bps: 0,
                    graduation_hook: None,
                    graduation_policy: GraduationPolicy::default(),
                    namespace: fixture.namespace,
                    launch_salt: SALT,
                    name: "Fixed".into(),
                    uri: "https://example.test/fixed".into(),
                    sale_inventory: 800,
                    graduation_reserve: 100,
                    creator_allocation: 200,
                    price: lbp_core::FixedPrice {
                        collateral_units: 3,
                        token_units: 2,
                    },
                    start: factory_core::SaleStart::Now { timestamp: 1 },
                    end_timestamp: Some(4),
                    allocation_release: factory_core::CreatorAllocationRelease::AfterSetup,
                    namespace_program_id: CURVE,
                    lbp_program_id: LBP,
                    collateral_definition_id: COLLATERAL,
                    creator,
                    now: 1,
                },
            )
        };
        let cycles = execute_create(&mut state, &signer, call, fixture.inclusion_timestamp_ms);
        let launch = LbpLaunchState::decode(
            state
                .get_account_by_id(fixture.factory)
                .data
                .actor_state(FACTORY),
        )
        .unwrap();
        assert_eq!(launch.creation_stage, LbpCreationStage::Complete);
        let pool = lbp_core::compute_pool_pda(
            fixture.namespace,
            LBP,
            fixture.definition,
            COLLATERAL,
            fixture.factory,
        );
        assert_eq!(launch.pool_id, pool);
        let saved =
            lbp_core::PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(LBP))
                .unwrap();
        let seed = if weighted { 100 } else { 0 };
        assert!(saved.funded);
        assert_eq!(saved.recorded_collateral, seed);
        assert_eq!(balance(&state, pool, COLLATERAL), seed);
        assert_eq!(balance(&state, creator, COLLATERAL), 1_000 - seed);
        assert_eq!(balance(&state, pool, fixture.definition), 800);
        assert_eq!(balance(&state, creator, fixture.definition), 200);
        eprintln!("SDK public LBP create (weighted={weighted}): {cycles} cycles");
    }
}
