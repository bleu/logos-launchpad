use super::*;

pub(super) const LBP_HOOK_PROGRAM: AccountId = AccountId::new([94; 32]);
pub(super) const LBP_FACTORY_PROGRAM: AccountId = AccountId::new([17; 32]);

pub(super) fn lbp_launch(env: &FixedFactoryEnv) -> factory_core::LbpLaunchState {
    factory_core::LbpLaunchState::decode(
        env.state
            .get_account_by_id(env.factory)
            .data
            .actor_state(LBP_FACTORY_PROGRAM),
    )
    .unwrap()
}

pub(super) fn lbp_adapter(env: &FixedFactoryEnv) -> Actor {
    Actor::new(
        factory_core::compute_graduation_pda(LBP_HOOK_PROGRAM, env.factory),
        LBP_HOOK_PROGRAM,
    )
}

pub(super) fn lbp_adapter_state(env: &FixedFactoryEnv) -> factory_core::AdapterState {
    factory_core::AdapterState::decode(
        env.state
            .get_account_by_id(lbp_adapter(env).account_id)
            .data
            .actor_state(LBP_HOOK_PROGRAM),
    )
    .unwrap()
}

pub(super) fn lbp_kind(weighted: bool) -> factory_core::GraduationIntentKind {
    if weighted {
        factory_core::GraduationIntentKind::Weighted
    } else {
        factory_core::GraduationIntentKind::FixedPrice
    }
}

pub(super) fn lbp_hook_message(creator: AccountId) -> factory_core::Message {
    factory_core::Message::ExecuteGraduationHook {
        creator_hint: Some(creator),
    }
}

pub(super) fn lbp_manual_message(weighted: bool, creator: AccountId) -> factory_core::Message {
    if weighted {
        factory_core::Message::WithdrawWeightedProceeds { creator }
    } else {
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 300 }
    }
}

pub(super) fn lbp_allocation_message(weighted: bool, creator: AccountId) -> factory_core::Message {
    if weighted {
        factory_core::Message::ClaimWeightedAllocation { creator }
    } else {
        factory_core::Message::ClaimCreatorAllocation { creator, now: 300 }
    }
}

pub(super) fn lbp_signed_call(
    env: &mut FixedFactoryEnv,
    receiver: Actor,
    instruction: impl borsh::BorshSerialize,
    signers: &[&PrivateKey],
    evidence: Vec<lee::PublicAccountEvidence>,
) -> Result<(), lee::error::LeeError> {
    let nonces: BTreeMap<_, _> = signers
        .iter()
        .map(|key| {
            let owner = AccountId::from(&PublicKey::new_from_private_key(key));
            (owner, env.state.get_account_by_id(owner).nonce)
        })
        .collect();
    let mut message = Message::try_new(receiver, env.actors.clone(), nonces, instruction).unwrap();
    message.admission_evidence = evidence;
    let witnesses = WitnessSet::for_message(&message, signers);
    env.state
        .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 300_000)
        .map(|_| ())
}

pub(super) fn initialize_lbp_adapter(
    env: &mut FixedFactoryEnv,
    accepted_intent: factory_core::GraduationIntentKind,
) {
    let adapter = lbp_adapter(env);
    env.actors.extend([
        adapter,
        Actor::new(adapter.account_id, TOKEN_PROGRAM_ACCOUNT_ID),
        holding(adapter.account_id, env.launch),
        holding(adapter.account_id, COLLATERAL),
    ]);
    let key = PrivateKey::try_new([96; 32]).unwrap();
    let caller = AccountId::from(&PublicKey::new_from_private_key(&key));
    env.actors.push(Actor::new(
        caller,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    let instruction = factory_core::AdapterMessage::Initialize {
        factory_program_id: LBP_FACTORY_PROGRAM,
        namespace: NAMESPACE,
        launch_salt: env.salt,
        accepted_intent,
    };
    let evidence = vec![lee::PublicAccountEvidence::Pda {
        program: LBP_HOOK_PROGRAM,
        seed: factory_core::compute_graduation_seed(env.factory),
    }];
    lbp_signed_call(env, adapter, instruction, &[&key], evidence).unwrap();
}

pub(super) fn lbp_manual_env(weighted: bool, buy: bool) -> FixedFactoryEnv {
    if weighted {
        let (state, factory, pool, launch, escrow, key, actors, salt) =
            weighted_factory_env_with_buy(buy);
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let (_, path) = allowlist_for(pool, creator);
        FixedFactoryEnv {
            state: clock_state(state, 300_000),
            factory,
            pool,
            launch,
            escrow,
            key,
            actors,
            salt,
            path,
        }
    } else {
        let mut env =
            fixed_factory_env(Some(300), factory_core::CreatorAllocationRelease::AfterEnd);
        if buy {
            env.buy(100, 156);
        }
        env.state = clock_state(env.state, 300_000);
        env
    }
}

pub(super) fn lbp_graduation_env(
    weighted: bool,
    buy: bool,
    configuration: Vec<u8>,
    accepted_intent: factory_core::GraduationIntentKind,
) -> FixedFactoryEnv {
    lbp_graduation_env_with_policy(
        weighted,
        buy,
        configuration,
        accepted_intent,
        test_graduation_policy(true),
    )
}

pub(super) fn lbp_graduation_env_with_policy(
    weighted: bool,
    buy: bool,
    configuration: Vec<u8>,
    accepted_intent: factory_core::GraduationIntentKind,
    graduation_policy: factory_core::GraduationPolicy,
) -> FixedFactoryEnv {
    lbp_graduation_env_with_adapter(
        weighted,
        buy,
        configuration,
        accepted_intent,
        graduation_policy,
        launchpad_methods::GRADUATION_FAULT_ADAPTER_ELF,
    )
}

fn lbp_graduation_env_with_adapter(
    weighted: bool,
    buy: bool,
    configuration: Vec<u8>,
    accepted_intent: factory_core::GraduationIntentKind,
    graduation_policy: factory_core::GraduationPolicy,
    adapter_elf: &'static [u8],
) -> FixedFactoryEnv {
    lbp_graduation_env_with_reserve(
        weighted,
        buy,
        configuration,
        accepted_intent,
        graduation_policy,
        adapter_elf,
        0,
    )
}

pub(super) fn lbp_graduation_env_with_reserve(
    weighted: bool,
    buy: bool,
    configuration: Vec<u8>,
    accepted_intent: factory_core::GraduationIntentKind,
    graduation_policy: factory_core::GraduationPolicy,
    adapter_elf: &'static [u8],
    graduation_reserve: u128,
) -> FixedFactoryEnv {
    let hook = Some(factory_core::GraduationHook {
        program_id: LBP_HOOK_PROGRAM,
        configuration,
    });
    let mut env = if weighted {
        let (state, factory, pool, launch, escrow, key, actors, salt) =
            weighted_factory_env_with_reserve(
                buy,
                hook,
                lbp_core::TradeMode::BuyOnly,
                0,
                graduation_policy,
                graduation_reserve,
            );
        let creator = AccountId::from(&PublicKey::new_from_private_key(&key));
        let (_, path) = allowlist_for(pool, creator);
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
    } else {
        let mut env = fixed_factory_env_with_reserve(
            Some(300),
            factory_core::CreatorAllocationRelease::AfterEnd,
            curve_core::PausePolicy::AuthorityControlled,
            lbp_core::EndingPolicy::Either,
            hook,
            lbp_core::TradeMode::BuyOnly,
            0,
            graduation_policy,
            graduation_reserve,
        );
        if buy {
            env.buy(100, 156);
        }
        env
    };
    env.state = env.state.with_programs_at(
        ([(
            LBP_HOOK_PROGRAM,
            Program::new(Cow::Borrowed(adapter_elf)).unwrap(),
        )])
        .map(|(address, program)| (address, program, true)),
    );
    initialize_lbp_adapter(&mut env, accepted_intent);
    let saved = lbp_launch(&env);
    assert_eq!(
        saved.graduation_hook.as_ref().unwrap().program_id,
        LBP_HOOK_PROGRAM
    );
    assert_eq!(
        saved.settlement_progress,
        lbp_core::SettlementProgress::Unselected
    );
    let pool = fixed_pool(&env.state, env.pool);
    assert_eq!(pool.graduation_adapter, Some(lbp_adapter(&env)));
    env.state = clock_state(env.state, 300_000);
    env
}

pub(super) fn donate_lbp_surplus(env: &mut FixedFactoryEnv) {
    let key = PrivateKey::try_new([23; 32]).unwrap();
    let creator = env.creator();
    let pool = env.pool;
    for (definition, amount) in [(COLLATERAL, 41), (env.launch, 7)] {
        lbp_signed_call(
            env,
            Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
            token::Message::Transfer {
                definition,
                recipient: pool,
                amount,
            },
            &[&key],
            vec![],
        )
        .unwrap();
    }
}

pub(super) fn reject_lbp_factory(
    env: &mut FixedFactoryEnv,
    instruction: factory_core::Message,
    timestamp: u64,
    reason: &str,
) {
    let before = env.state.clone();
    let creator = env.creator();
    let nonce = env.state.get_account_by_id(creator).nonce;
    let result = env.execute(instruction, timestamp);
    assert!(
        matches!(&result, Err(lee::error::LeeError::ProgramExecutionFailed(error)) if error.contains(reason)),
        "expected {reason}, got {result:?}"
    );
    assert!(
        env.state == before,
        "rejected LBP graduation changed runtime state"
    );
    assert_eq!(env.state.get_account_by_id(creator).nonce, nonce);
}

pub(super) fn assert_lbp_graduated(
    env: &FixedFactoryEnv,
    weighted: bool,
    initial: &lbp_core::PoolAccount,
    creator_collateral: u128,
    supply: u128,
    surplus: bool,
    allocation_claimed: bool,
) {
    let creator = env.creator();
    let adapter = lbp_adapter(env).account_id;
    let launch_share =
        initial.remaining_inventory * u128::from(initial.graduation_policy.launch_bps) / 10_000;
    let collateral_share =
        initial.recorded_collateral * u128::from(initial.graduation_policy.collateral_bps) / 10_000;
    let mut expected = initial.clone();
    expected.closure_observed = true;
    expected.recorded_collateral = 0;
    expected.accrued_fees = 0;
    expected.settlement_progress = lbp_core::SettlementProgress::Graduated;
    assert_eq!(fixed_pool(&env.state, env.pool), expected);
    let saved = lbp_launch(env);
    assert_eq!(
        saved.settlement_progress,
        lbp_core::SettlementProgress::Graduated
    );
    assert_eq!(saved.creator_allocation_released, allocation_claimed);
    assert_eq!(
        balance(&env.state, creator, COLLATERAL),
        creator_collateral + initial.recorded_collateral - collateral_share
    );
    assert_eq!(balance(&env.state, adapter, COLLATERAL), collateral_share);
    assert_eq!(balance(&env.state, adapter, env.launch), launch_share);
    assert_eq!(
        balance(&env.state, NEXT_TREASURY, COLLATERAL),
        initial.accrued_fees
    );
    assert_eq!(balance(&env.state, TREASURY, COLLATERAL), 0);
    assert_eq!(
        balance(&env.state, env.pool, COLLATERAL),
        if surplus { 41 } else { 0 }
    );
    assert_eq!(
        balance(&env.state, env.pool, env.launch),
        if surplus { 7 } else { 0 }
    );
    assert_eq!(
        balance(&env.state, env.escrow, env.launch),
        if allocation_claimed { 0 } else { 200 }
    );
    assert_eq!(token_supply(&env.state, env.launch), supply);
    let receipt = lbp_adapter_state(env).receipt.unwrap();
    assert_eq!(
        receipt,
        factory_core::GraduationRequest {
            factory_program_id: LBP_FACTORY_PROGRAM,
            namespace: NAMESPACE,
            launch_salt: env.salt,
            token_definition_id: env.launch,
            collateral_definition_id: COLLATERAL,
            intent: if weighted {
                factory_core::GraduationIntent::Weighted {
                    launch_tokens: launch_share,
                    collateral: collateral_share,
                }
            } else {
                factory_core::GraduationIntent::FixedPrice {
                    launch_tokens: launch_share,
                    collateral: collateral_share,
                }
            },
            reserve_tokens: 0,
            unsold_tokens: launch_share,
            configuration: saved.graduation_hook.unwrap().configuration,
        }
    );
    assert_eq!(receipt.intent.amounts(), (launch_share, collateral_share));
    assert_eq!(receipt.intent.kind(), lbp_kind(weighted));
}
