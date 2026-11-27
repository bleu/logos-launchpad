use crate::fixture::{
    COLLATERAL, CURVE, FACTORY, FactoryFixture, SALT, account, balance, factory_fixture, guest,
    holding, id, key,
};
use factory_core::{
    CreationStage, GraduationPolicy, LbpCreationStage, LbpLaunchState, Message as FactoryMessage,
};
use launchpad_client::{
    CreateSaleRequest, FactorySale, PublicInvocation, build_create_fixed_price_invocation,
    build_create_sale_invocation, build_create_weighted_invocation,
};
use lee::{
    AccountId, PrivateKey, PublicTransaction, V03State,
    public_transaction::{Message, WitnessSet},
};
use lez_compat_core::token::{TokenKind, TokenState};
use std::collections::BTreeMap;

const LBP: AccountId = AccountId::new([14; 32]);
const CYCLE_BUDGET: u64 = 10_000_000;

fn signed_create<I: borsh::BorshSerialize>(
    state: &V03State,
    signer: &PrivateKey,
    call: &PublicInvocation<I>,
) -> PublicTransaction {
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
    PublicTransaction::new(message, witnesses)
}

fn meter_create(
    state: &V03State,
    transaction: &PublicTransaction,
    timestamp_ms: u64,
) -> (lee::ExecutionCharge, lee::ValidatedStateDiff) {
    let (charge, diff) = lee::ValidatedStateDiff::from_public_transaction_metered(
        transaction,
        state,
        1,
        timestamp_ms,
        CYCLE_BUDGET,
    );
    (
        charge,
        diff.expect("signed creation must return an applicable diff"),
    )
}

fn execute_create<I: borsh::BorshSerialize>(
    state: &mut V03State,
    signer: &PrivateKey,
    call: &PublicInvocation<I>,
    timestamp_ms: u64,
) -> u64 {
    let creator = id(signer);
    let nonce = state.get_account_by_id(creator).nonce;
    let transaction = signed_create(state, signer, call);
    let (charge, diff) = meter_create(state, &transaction, timestamp_ms);
    assert!(
        charge.revert.is_none(),
        "create reverted: {:?}",
        charge.revert
    );
    state.apply_state_diff(diff).unwrap();
    assert_eq!(state.get_account_by_id(creator).nonce.0, nonce.0 + 1);
    charge.cycles
}

fn assert_create_reverts(
    state: &mut V03State,
    signer: &PrivateKey,
    call: &PublicInvocation<FactoryMessage>,
    timestamp_ms: u64,
    reason: &str,
) {
    let transaction = signed_create(state, signer, call);
    let (charge, diff) = meter_create(state, &transaction, timestamp_ms);
    assert!(
        matches!(&charge.revert, Some(message) if message.contains(reason)),
        "expected {reason}, got {:?}",
        charge.revert
    );
    let creator = id(signer);
    let mut signer_account = state.get_account_by_id(creator);
    signer_account.nonce.0 += 1;
    let expected = state
        .clone()
        .with_public_accounts([(creator, signer_account)]);
    assert!(state.apply_state_diff(diff).unwrap().is_empty());
    assert!(
        *state == expected,
        "reverted creation changed more than the signer nonce"
    );
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
        &call,
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

fn lbp_fixture() -> FactoryFixture {
    let mut fixture = factory_fixture();
    fixture.state = fixture
        .state
        .with_programs_at([(LBP, guest("LAUNCHPAD_LBP_ELF"), true)]);
    fixture
}

fn fixed_create(namespace: AccountId, creator: AccountId) -> PublicInvocation<FactoryMessage> {
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
            namespace,
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
}

fn weighted_create(
    namespace: AccountId,
    creator: AccountId,
    collateral_seed: u128,
) -> PublicInvocation<FactoryMessage> {
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
            namespace,
            launch_salt: SALT,
            name: "Weighted".into(),
            uri: "https://example.test/weighted".into(),
            sale_inventory: 800,
            graduation_reserve: 100,
            creator_allocation: 200,
            collateral_seed,
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
}

fn assert_lbp_funded(
    fixture: &FactoryFixture,
    creator: AccountId,
    seed: u128,
    creator_collateral_left: u128,
) {
    let state = &fixture.state;
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
        lbp_core::PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(LBP)).unwrap();
    assert!(saved.funded);
    assert_eq!(saved.remaining_inventory, 800);
    assert_eq!(saved.recorded_collateral, seed);
    if seed == 0 {
        assert_eq!(saved.mode, lbp_core::PoolMode::FixedPrice);
    } else {
        assert!(matches!(
            saved.mode,
            lbp_core::PoolMode::Weighted {
                collateral_seed,
                pricing_collateral,
                ..
            } if collateral_seed == seed && pricing_collateral == seed
        ));
    }
    assert_eq!(balance(state, pool, COLLATERAL), seed);
    assert_eq!(balance(state, creator, COLLATERAL), creator_collateral_left);
    assert_eq!(balance(state, pool, fixture.definition), 800);
    assert_eq!(balance(state, creator, fixture.definition), 200);
    let mint = factory_core::compute_mint_pda(fixture.namespace, FACTORY, SALT);
    assert_eq!(balance(state, mint, fixture.definition), 100);
    let escrow = factory_core::compute_escrow_pda(fixture.namespace, FACTORY, SALT);
    assert_eq!(balance(state, escrow, fixture.definition), 0);
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn sdk_lbp_creates_fund_inventory_and_weighted_seed() {
    let signer = key(48);
    let creator = id(&signer);
    let mut fixture = lbp_fixture();
    let before = fixture.state.clone();
    for (call, seed, creator_collateral_left) in [
        (fixed_create(fixture.namespace, creator), 0, 1_000),
        (weighted_create(fixture.namespace, creator, 100), 100, 900),
    ] {
        fixture.state = before.clone();
        let cycles = execute_create(
            &mut fixture.state,
            &signer,
            &call,
            fixture.inclusion_timestamp_ms,
        );
        assert_lbp_funded(&fixture, creator, seed, creator_collateral_left);
        eprintln!("SDK public LBP create (seed={seed}): {cycles} cycles");
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn sdk_lbp_creation_outside_inclusion_window_rolls_back() {
    let signer = key(48);
    let creator = id(&signer);
    let fixture = lbp_fixture();
    for (call, rejected_ms, accepted_ms) in [
        (fixed_create(fixture.namespace, creator), 4_000, 3_999),
        (
            weighted_create(fixture.namespace, creator, 100),
            2_001,
            2_000,
        ),
    ] {
        let mut state = fixture.state.clone();
        assert_create_reverts(
            &mut state,
            &signer,
            &call,
            rejected_ms,
            &lee::error::LeeError::OutOfValidityWindow.to_string(),
        );
        execute_create(&mut state, &signer, &call, accepted_ms);
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn sdk_lbp_creation_with_wrong_factory_binding_rolls_back() {
    let signer = key(48);
    let creator = id(&signer);
    let mut fixture = lbp_fixture();
    for mut call in [
        fixed_create(fixture.namespace, creator),
        weighted_create(fixture.namespace, creator, 100),
    ] {
        let salt = match &mut call.instruction {
            FactoryMessage::CreateFixedPriceLbp { inputs } => &mut inputs.launch_salt,
            FactoryMessage::CreateWeightedLbp { inputs } => &mut inputs.launch_salt,
            _ => unreachable!(),
        };
        // Keep the builder's destination and PDA evidence bound to the original salt.
        salt[0] ^= 1;
        assert_create_reverts(
            &mut fixture.state,
            &signer,
            &call,
            fixture.inclusion_timestamp_ms,
            "WrongAccount",
        );
    }
}

#[test]
#[ignore = "requires fresh application and compatibility ELFs and the pinned clock ELF"]
fn sdk_weighted_creation_without_full_creator_seed_rolls_back_and_can_retry() {
    let signer = key(48);
    let creator = id(&signer);
    let mut fixture = lbp_fixture();
    assert_eq!(balance(&fixture.state, creator, COLLATERAL), 1_000);
    let call = weighted_create(fixture.namespace, creator, 1_001);
    assert_create_reverts(
        &mut fixture.state,
        &signer,
        &call,
        fixture.inclusion_timestamp_ms,
        "InvalidCustody",
    );

    fixture.state = fixture.state.with_public_accounts([account(
        holding(creator, COLLATERAL),
        borsh::to_vec(&TokenState::Holding {
            definition: COLLATERAL,
            owner: creator,
            kind: TokenKind::Fungible,
            balance: 1_001,
        })
        .unwrap(),
    )]);
    execute_create(
        &mut fixture.state,
        &signer,
        &call,
        fixture.inclusion_timestamp_ms,
    );
    assert_lbp_funded(&fixture, creator, 1_001, 0);
}
