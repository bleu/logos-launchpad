use super::*;

/// Complete weighted setup and one buy for controls and settlement tests.
pub(super) fn weighted_factory_env() -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    weighted_factory_env_with_buy(true)
}

pub(super) fn weighted_factory_env_with_buy(
    execute_buy: bool,
) -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    weighted_factory_env_with_hook(execute_buy, None)
}

fn weighted_factory_env_with_hook(
    execute_buy: bool,
    graduation_hook: Option<factory_core::GraduationHook>,
) -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    weighted_factory_env_with_trade_mode(execute_buy, graduation_hook, lbp_core::TradeMode::BuyOnly)
}

pub(super) fn weighted_factory_env_with_trade_mode(
    execute_buy: bool,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
) -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    weighted_factory_env_with_sell_fee(execute_buy, graduation_hook, trade_mode, 0)
}

pub(super) fn weighted_factory_env_with_sell_fee(
    execute_buy: bool,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
    sell_fee_bps: u16,
) -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    let policy = test_graduation_policy(graduation_hook.is_some());
    weighted_factory_env_with_sell_fee_and_policy(
        execute_buy,
        graduation_hook,
        trade_mode,
        sell_fee_bps,
        policy,
    )
}

fn weighted_factory_env_with_sell_fee_and_policy(
    execute_buy: bool,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
    sell_fee_bps: u16,
    graduation_policy: factory_core::GraduationPolicy,
) -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    weighted_factory_env_with_reserve(
        execute_buy,
        graduation_hook,
        trade_mode,
        sell_fee_bps,
        graduation_policy,
        0,
    )
}

pub(super) fn weighted_factory_env_with_reserve(
    execute_buy: bool,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
    sell_fee_bps: u16,
    graduation_policy: factory_core::GraduationPolicy,
    graduation_reserve: u128,
) -> (
    V03State,
    AccountId, // factory
    AccountId, // pool
    AccountId, // launch (token definition)
    AccountId, // escrow
    PrivateKey,
    Vec<Actor>, // actors for factory transactions
    [u8; 32],   // salt
) {
    use factory_core::{CreatorAllocationRelease, WeightedInputs};
    const FACTORY: AccountId = AccountId::new([17; 32]);
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
    let salt = [24; 32];
    let factory = factory_core::compute_factory_pda(NAMESPACE, FACTORY, salt);
    let launch = factory_core::compute_definition_pda(NAMESPACE, FACTORY, salt);
    let mint = factory_core::compute_mint_pda(NAMESPACE, FACTORY, salt);
    let escrow = factory_core::compute_escrow_pda(NAMESPACE, FACTORY, salt);
    let role = factory_core::compute_creator_role_pda(NAMESPACE, FACTORY, salt);
    let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, launch, COLLATERAL, factory);
    let authority = factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID;
    let mut state = V03State::new()
        .with_programs_at(
            [
                (
                    FACTORY,
                    Program::new(Cow::Borrowed(launchpad_methods::FACTORY_ELF)).unwrap(),
                ),
                (
                    LBP,
                    Program::new(Cow::Borrowed(launchpad_methods::LBP_ELF)).unwrap(),
                ),
                (
                    CURVE,
                    Program::new(Cow::Borrowed(launchpad_methods::CURVE_ELF)).unwrap(),
                ),
                (
                    TOKEN_PROGRAM_ACCOUNT_ID,
                    Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF)).unwrap(),
                ),
                (
                    authority,
                    Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF)).unwrap(),
                ),
                (clock_core::clock_account_id(), programs::clock()),
            ]
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts([
            account(
                Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
                borsh::to_vec(&TokenState::Definition {
                    name: "Collateral".into(),
                    kind: TokenKind::Fungible,
                    supply: 10_000,
                    metadata: None,
                })
                .unwrap(),
            ),
            account(
                holding(creator, COLLATERAL),
                borsh::to_vec(&TokenState::Holding {
                    owner: creator,
                    definition: COLLATERAL,
                    kind: TokenKind::Fungible,
                    balance: 1000,
                })
                .unwrap(),
            ),
            account(
                holding(TREASURY, COLLATERAL),
                borsh::to_vec(&TokenState::Holding {
                    owner: TREASURY,
                    definition: COLLATERAL,
                    kind: TokenKind::Fungible,
                    balance: 0,
                })
                .unwrap(),
            ),
            account(
                holding(NEXT_TREASURY, COLLATERAL),
                borsh::to_vec(&TokenState::Holding {
                    owner: NEXT_TREASURY,
                    definition: COLLATERAL,
                    kind: TokenKind::Fungible,
                    balance: 0,
                })
                .unwrap(),
            ),
        ]);
    state = fixed_config(clock_state(state, 100_000), 100, TREASURY, false);
    let mut actors = vec![
        Actor::new(factory, FACTORY),
        Actor::new(pool, LBP),
        Actor::new(role, authority),
        Actor::new(creator, authority),
        Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
        holding(TREASURY, launch),
        holding(NEXT_TREASURY, launch),
        holding(TREASURY, COLLATERAL),
        holding(NEXT_TREASURY, COLLATERAL),
    ];
    for owner in [factory, mint, escrow, pool, creator, role, launch] {
        actors.push(Actor::new(owner, FACTORY));
        actors.push(Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID));
        actors.push(holding(owner, launch));
        actors.push(holding(owner, COLLATERAL));
    }
    factory_execute_at(
        &mut state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::CreateWeightedLbp {
            inputs: WeightedInputs {
                per_block_token_limit: None,
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy: curve_core::PausePolicy::AuthorityControlled,
                ending_policy: lbp_core::EndingPolicy::Scheduled,
                trade_mode,
                sell_fee_bps,
                graduation_policy,
                graduation_hook,
                namespace: NAMESPACE,
                launch_salt: salt,
                name: "Weighted".into(),
                uri: "https://example.test/weighted".into(),
                graduation_reserve,
                sale_inventory: 800,
                creator_allocation: 200,
                collateral_seed: 100,
                token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
                token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
                start_timestamp: 150,
                end_timestamp: 300,
                allocation_release: CreatorAllocationRelease::AfterEnd,
                namespace_program_id: CURVE,
                lbp_program_id: LBP,
                collateral_definition_id: COLLATERAL,
                creator,
            },
        },
        100_000,
    )
    .unwrap();
    assert!(fixed_pool(&state, pool).funded);
    assert_eq!(fixed_pool(&state, pool).sell_fee_bps, sell_fee_bps);
    assert_eq!(
        factory_core::LbpLaunchState::decode(
            state.get_account_by_id(factory).data.actor_state(FACTORY)
        )
        .unwrap()
        .sell_fee_bps,
        sell_fee_bps
    );
    assert_eq!(
        fixed_pool(&state, pool).settlement_progress,
        lbp_core::SettlementProgress::Unselected,
    );
    state = clock_state(state, 150_000);
    if execute_buy {
        fixed_execute_at(
            &mut state,
            pool,
            &key,
            lbp_core::Message::BuyWeightedExactInput {
                membership: None,
                amount_in: 50,
                min_amount_out: 1,
            },
            150_000,
        )
        .unwrap();
    }
    (state, factory, pool, launch, escrow, key, actors, salt)
}
