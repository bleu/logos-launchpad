use super::*;

pub(super) fn fixed_config(
    state: V03State,
    fee_bps: u16,
    treasury: AccountId,
    frozen: bool,
) -> V03State {
    let config = borsh::to_vec(&(
        *b"LZNSCONF",
        3u16,
        NAMESPACE,
        fee_bps,
        treasury,
        (AccountId::new([13; 32]), frozen),
    ))
    .unwrap();
    curve_core::Config::decode(&config).unwrap();
    state.with_public_accounts([account(
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        config,
    )])
}

pub(super) fn fixed_state(buyer: AccountId, end: Option<u64>) -> (V03State, AccountId) {
    let pool = lbp_core::compute_pool_pda(NAMESPACE, LBP, LAUNCH, COLLATERAL, OWNER);
    // Seeded funded state does not test the four setup stages.
    let bytes = borsh::to_vec(&(
        (None::<u128>, None::<u64>, 0u128),
        (
            curve_core::BuyPolicy::Open,
            lbp_core::TradeControl::Unrestricted,
            if end.is_some() {
                lbp_core::EndingPolicy::Either
            } else {
                lbp_core::EndingPolicy::Depletion
            },
            lbp_core::TradeMode::BuyOnly,
            0u16,
            None::<Actor>,
            factory_core::GraduationPolicy::default(),
            lbp_core::PoolMode::FixedPrice,
            true,
            false,
            NAMESPACE,
            CURVE,
            OWNER,
            AccountId::new([17; 32]),
            [18u8; 32],
            LAUNCH,
            COLLATERAL,
        ),
        (
            1_000u128,
            1_000u128,
            0u128,
            lbp_core::FixedPrice {
                collateral_units: 3,
                token_units: 2,
            },
            100u64,
            end,
            lbp_core::CreatorAllocationRelease::AfterEnd,
            0u128,
            0u128,
            lbp_core::SettlementProgress::Unselected,
            None::<(bool, u8)>,
        ),
    ))
    .unwrap();
    lbp_core::PoolAccount::decode(&bytes).unwrap();
    let mut accounts = vec![account(Actor::new(pool, LBP), bytes)];
    for (owner, definition, balance) in [
        (pool, LAUNCH, 1_007),
        (pool, COLLATERAL, 41),
        (buyer, LAUNCH, 0),
        (buyer, COLLATERAL, 2_000),
        (TREASURY, COLLATERAL, 0),
        (NEXT_TREASURY, COLLATERAL, 0),
    ] {
        accounts.push(account(
            holding(owner, definition),
            borsh::to_vec(&TokenState::Holding {
                definition,
                owner,
                kind: TokenKind::Fungible,
                balance,
            })
            .unwrap(),
        ));
    }
    let state = V03State::new()
        .with_programs_at(
            [
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
                (clock_core::clock_account_id(), programs::clock()),
            ]
            .map(|(address, program)| (address, program, true)),
        )
        .with_public_accounts(accounts);
    (
        clock_state(fixed_config(state, 333, TREASURY, false), 100_000),
        pool,
    )
}

pub(super) fn fixed_pool(state: &V03State, pool: AccountId) -> lbp_core::PoolAccount {
    lbp_core::PoolAccount::decode(state.get_account_by_id(pool).data.actor_state(LBP)).unwrap()
}

pub(super) fn weighted_state(buyer: AccountId) -> (V03State, AccountId) {
    let (state, pool) = fixed_state(buyer, Some(200));
    let mut sale = fixed_pool(&state, pool);
    sale.trade_control = lbp_core::TradeControl::Running;
    sale.ending_policy = lbp_core::EndingPolicy::Scheduled;
    sale.mode = lbp_core::PoolMode::Weighted {
        token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
        collateral_seed: 100,
        pricing_collateral: 100,
        checkpoint_time: 0,
        checkpoint_token_weight: 0,
    };
    sale.price = lbp_core::FixedPrice {
        collateral_units: 0,
        token_units: 0,
    };
    sale.recorded_collateral = 100;
    let state = state.with_public_accounts([
        account(Actor::new(pool, LBP), borsh::to_vec(&sale).unwrap()),
        account(
            holding(pool, COLLATERAL),
            borsh::to_vec(&TokenState::Holding {
                definition: COLLATERAL,
                owner: pool,
                kind: TokenKind::Fungible,
                // Include surplus. Only the recorded 100 units enter pricing.
                balance: 141,
            })
            .unwrap(),
        ),
    ]);
    (state, pool)
}
