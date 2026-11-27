use super::*;

pub(super) struct FixedFactoryEnv {
    pub(super) state: V03State,
    pub(super) factory: AccountId,
    pub(super) pool: AccountId,
    pub(super) launch: AccountId,
    pub(super) escrow: AccountId,
    pub(super) key: PrivateKey,
    pub(super) actors: Vec<Actor>,
    pub(super) salt: [u8; 32],
    pub(super) path: curve_core::MembershipPath,
}

impl FixedFactoryEnv {
    pub(super) fn creator(&self) -> AccountId {
        AccountId::from(&PublicKey::new_from_private_key(&self.key))
    }

    pub(super) fn execute(
        &mut self,
        instruction: factory_core::Message,
        timestamp: u64,
    ) -> Result<(), lee::error::LeeError> {
        factory_execute_at(
            &mut self.state,
            self.factory,
            &self.actors,
            &self.key,
            self.salt,
            self.launch,
            self.pool,
            instruction,
            timestamp,
        )
    }

    pub(super) fn execute_events(
        &mut self,
        instruction: factory_core::Message,
        timestamp: u64,
    ) -> Result<Vec<lee_core::program::TransactionEvent>, lee::error::LeeError> {
        factory_execute_events_at(
            &mut self.state,
            self.factory,
            &self.actors,
            &self.key,
            self.salt,
            self.launch,
            self.pool,
            instruction,
            timestamp,
        )
    }

    pub(super) fn buy(&mut self, amount_out: u128, max_amount_in: u128) {
        fixed_execute_at(
            &mut self.state,
            self.pool,
            &self.key,
            lbp_core::Message::BuyExactOutput {
                membership: Some(self.path.clone()),
                amount_out,
                max_amount_in,
                now: 100,
            },
            100_000,
        )
        .unwrap();
    }
}

pub(super) fn fixed_factory_env(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
) -> FixedFactoryEnv {
    fixed_factory_env_with_policy(end, release, curve_core::PausePolicy::Disabled)
}

pub(super) fn fixed_factory_env_with_policy(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
) -> FixedFactoryEnv {
    fixed_factory_env_with_hook(end, release, pause_policy, None)
}

pub(super) fn fixed_factory_env_with_ending(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    ending_policy: lbp_core::EndingPolicy,
) -> FixedFactoryEnv {
    fixed_factory_env_with_options(end, release, pause_policy, ending_policy, None)
}

fn fixed_factory_env_with_hook(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    graduation_hook: Option<factory_core::GraduationHook>,
) -> FixedFactoryEnv {
    fixed_factory_env_with_options(
        end,
        release,
        pause_policy,
        if end.is_some() {
            lbp_core::EndingPolicy::Either
        } else {
            lbp_core::EndingPolicy::Depletion
        },
        graduation_hook,
    )
}

fn fixed_factory_env_with_options(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    ending_policy: lbp_core::EndingPolicy,
    graduation_hook: Option<factory_core::GraduationHook>,
) -> FixedFactoryEnv {
    fixed_factory_env_with_trade_mode(
        end,
        release,
        pause_policy,
        ending_policy,
        graduation_hook,
        lbp_core::TradeMode::BuyOnly,
    )
}

pub(super) fn fixed_factory_env_with_trade_mode(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    ending_policy: lbp_core::EndingPolicy,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
) -> FixedFactoryEnv {
    fixed_factory_env_with_sell_fee(
        end,
        release,
        pause_policy,
        ending_policy,
        graduation_hook,
        trade_mode,
        0,
    )
}

pub(super) fn fixed_factory_env_with_sell_fee(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    ending_policy: lbp_core::EndingPolicy,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
    sell_fee_bps: u16,
) -> FixedFactoryEnv {
    let policy = test_graduation_policy(graduation_hook.is_some());
    fixed_factory_env_with_sell_fee_and_policy(
        end,
        release,
        pause_policy,
        ending_policy,
        graduation_hook,
        trade_mode,
        sell_fee_bps,
        policy,
    )
}

#[allow(clippy::too_many_arguments)]
fn fixed_factory_env_with_sell_fee_and_policy(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    ending_policy: lbp_core::EndingPolicy,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
    sell_fee_bps: u16,
    graduation_policy: factory_core::GraduationPolicy,
) -> FixedFactoryEnv {
    fixed_factory_env_with_reserve(
        end,
        release,
        pause_policy,
        ending_policy,
        graduation_hook,
        trade_mode,
        sell_fee_bps,
        graduation_policy,
        0,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn fixed_factory_env_with_reserve(
    end: Option<u64>,
    release: factory_core::CreatorAllocationRelease,
    pause_policy: curve_core::PausePolicy,
    ending_policy: lbp_core::EndingPolicy,
    graduation_hook: Option<factory_core::GraduationHook>,
    trade_mode: lbp_core::TradeMode,
    sell_fee_bps: u16,
    graduation_policy: factory_core::GraduationPolicy,
    graduation_reserve: u128,
) -> FixedFactoryEnv {
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
    let (policy, path) = allowlist_for(pool, creator);
    let mut state = fixed_config(
        clock_state(
            V03State::new()
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
                            Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_TOKEN_ELF))
                                .unwrap(),
                        ),
                        (
                            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                            Program::new(Cow::Borrowed(lez_compat_methods::COMPAT_AUTHORITY_ELF))
                                .unwrap(),
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
                            balance: 1_000,
                        })
                        .unwrap(),
                    ),
                ]),
            100_000,
        ),
        333,
        TREASURY,
        false,
    );
    let mut actors = vec![
        Actor::new(factory, FACTORY),
        Actor::new(pool, LBP),
        Actor::new(role, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(creator, factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(curve_core::compute_config_pda(NAMESPACE, CURVE), CURVE),
        Actor::new(launch, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(COLLATERAL, TOKEN_PROGRAM_ACCOUNT_ID),
        Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            clock_core::clock_account_id(),
        ),
    ];
    for owner in [factory, mint, escrow, pool, creator, role, launch] {
        actors.extend([
            Actor::new(owner, FACTORY),
            Actor::new(owner, LBP),
            Actor::new(owner, TOKEN_PROGRAM_ACCOUNT_ID),
            holding(owner, launch),
            holding(owner, COLLATERAL),
        ]);
    }
    let transaction = factory_transaction(
        &state,
        factory,
        &actors,
        &key,
        salt,
        launch,
        pool,
        factory_core::Message::CreateFixedPriceLbp {
            inputs: factory_core::FixedPriceInputs {
                per_block_token_limit: None,
                buy_policy: policy.clone(),
                pause_policy,
                ending_policy,
                trade_mode,
                sell_fee_bps,
                graduation_policy,
                graduation_hook,
                namespace: NAMESPACE,
                launch_salt: salt,
                name: "Fixed".into(),
                uri: "https://example.test/fixed".into(),
                graduation_reserve,
                sale_inventory: 800,
                creator_allocation: 200,
                price: lbp_core::FixedPrice {
                    collateral_units: 3,
                    token_units: 2,
                },
                start: factory_core::SaleStart::Now { timestamp: 100 },
                end_timestamp: end,
                allocation_release: release,
                namespace_program_id: CURVE,
                lbp_program_id: LBP,
                collateral_definition_id: COLLATERAL,
                creator,
                now: 100,
            },
        },
    );
    let nonce = state.get_account_by_id(creator).nonce;
    let (diff, charge) = lee::ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &transaction,
        &state,
        1,
        100_000,
        10_000_000,
    )
    .unwrap();
    state.apply_state_diff(diff).unwrap();
    let stored = factory_core::LbpLaunchState::decode(
        state.get_account_by_id(factory).data.actor_state(FACTORY),
    )
    .unwrap();
    assert_eq!(
        stored.creation_stage,
        factory_core::LbpCreationStage::Complete
    );
    assert_eq!(stored.buy_policy, policy);
    assert_eq!(stored.ending_policy, ending_policy);
    assert_eq!(stored.sell_fee_bps, sell_fee_bps);
    let sale = fixed_pool(&state, pool);
    assert!(sale.funded);
    assert_eq!(sale.price.collateral_units, 3);
    assert_eq!(sale.price.token_units, 2);
    assert_eq!(sale.recorded_collateral, 0);
    assert_eq!(balance(&state, pool, COLLATERAL), 0);
    assert_eq!(balance(&state, pool, launch), 800);
    assert_eq!(balance(&state, mint, launch), graduation_reserve);
    let released = release == factory_core::CreatorAllocationRelease::AfterSetup;
    if released {
        assert_eq!(balance(&state, creator, launch), 200);
    } else {
        assert!(
            state
                .get_account_by_id(holding(creator, launch).account_id)
                .data
                .actor_state(TOKEN_PROGRAM_ACCOUNT_ID)
                .is_empty()
        );
    }
    assert_eq!(
        balance(&state, escrow, launch),
        if released { 0 } else { 200 }
    );
    assert_eq!(state.get_account_by_id(creator).nonce.0, nonce.0 + 1);
    eprintln!(
        "single public fixed-price setup: {} cycles, {} signed transaction bytes",
        charge.cycles,
        borsh::object_length(&transaction).unwrap()
    );
    for treasury in [TREASURY, NEXT_TREASURY] {
        actors.push(holding(treasury, COLLATERAL));
        state = state.with_public_accounts([token_holding(treasury, COLLATERAL, 0)]);
    }
    FixedFactoryEnv {
        state,
        factory,
        pool,
        launch,
        escrow,
        key,
        actors,
        salt,
        path,
    }
}

pub(super) fn fixed_settlement_env(end: Option<u64>) -> FixedFactoryEnv {
    let mut env = fixed_factory_env(end, factory_core::CreatorAllocationRelease::AfterEnd);
    env.buy(100, 156);
    let creator = env.creator();
    for (definition, amount) in [(COLLATERAL, 41), (env.launch, 7)] {
        let message = Message::try_new(
            Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
            env.actors.clone(),
            BTreeMap::from([(creator, env.state.get_account_by_id(creator).nonce)]),
            token::Message::Transfer {
                definition,
                recipient: env.pool,
                amount,
            },
        )
        .unwrap();
        let witnesses = WitnessSet::for_message(&message, &[&env.key]);
        env.state
            .transition_from_public_transaction(
                &PublicTransaction::new(message, witnesses),
                1,
                100_000,
            )
            .unwrap();
    }
    env
}
