use super::fixture::{
    HOLDER, NS, OWNER, PROGRAM, TOKEN0, TOKEN1, answer_reads, call, create_message, namespace,
    pool_id, pool_state, start,
};
use crate::{
    Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID, compute_config_pda, compute_pool_pda, handle,
};
use lee_core::account::{AccountId, Actor, ActorState};
use lee_core::program::ReceiveInput;

#[test]
fn direct_creation_requires_the_creation_identity_signer() {
    assert!(matches!(
        handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(HOLDER, PROGRAM)),
            create_message(false),
        )),
        Err(crate::Error::Unauthorized)
    ));
}

#[test]
fn direct_creator_bootstraps_a_transfer_only_role_and_funds_reserves() {
    let pending = handle(&call(
        pool_id(),
        ActorState::empty(),
        Some(Actor::new(OWNER, PROGRAM)),
        create_message(false),
    ))
    .unwrap();
    assert_eq!(
        pending.sync_messages[0].to,
        Actor::new(compute_config_pda(NS, PROGRAM), PROGRAM)
    );
    let response = answer_reads(pending, &pool_state(), namespace(), [0, 0]).unwrap();
    let state = PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.owner, OWNER);
    assert!(state.funded);
    assert_eq!(
        (state.pool.real_reserve0, state.pool.real_reserve1),
        (800, 25)
    );
    let bootstrap = response.sync_messages.last().unwrap();
    assert_eq!(
        bootstrap.to.program_account_id,
        crate::AUTHORITY_PROGRAM_ACCOUNT_ID
    );
    assert_eq!(
        borsh::from_slice::<lez_compat_core::authority::Message>(&bootstrap.message).unwrap(),
        lez_compat_core::authority::Message::BootstrapTransferOnly { authority: OWNER }
    );
    assert_eq!(bootstrap.pda_seeds.len(), 1);
    assert_eq!(
        AccountId::for_public_pda(&PROGRAM, bootstrap.pda_seeds.first().unwrap()),
        bootstrap.to.account_id
    );
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter(|m| m.to == Actor::new(OWNER, TOKEN_PROGRAM_ACCOUNT_ID))
        .collect();
    assert_eq!(transfers.len(), 2);
}

#[test]
fn zero_reserves_initialize_both_holdings_without_zero_transfers() {
    let mut message = create_message(false);
    if let Message::CreatePool { token1_amount, .. } = &mut message {
        *token1_amount = 0;
    }
    let response = handle(&call(
        pool_id(),
        ActorState::empty(),
        Some(Actor::new(OWNER, PROGRAM)),
        message,
    ))
    .unwrap();
    let response = answer_reads(response, &pool_state(), namespace(), [0, 0]).unwrap();
    let calls: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .collect();
    assert_eq!(
        calls,
        vec![
            lez_compat_core::token::Message::Initialize { owner: pool_id() },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: pool_id(),
                amount: 800
            },
            lez_compat_core::token::Message::Initialize { owner: pool_id() }
        ]
    );
    assert!(
        PoolAccount::decode(&response.post_state.unwrap())
            .unwrap()
            .funded
    );
}

#[test]
fn creation_rejects_invalid_bounds_sides_program_fields_and_existing_state() {
    let message = create_message(false);
    let mut cases = vec![];
    for field in 0..9 {
        let mut invalid = message.clone();
        if let Message::CreatePool {
            token0_amount,
            virtual_reserve0,
            allowed_input_token,
            close_on_depletion,
            owner_program_id,
            owner_program_seed,
            curve_program_id,
            close_timestamp,
            token1_definition_id,
            ..
        } = &mut invalid
        {
            match field {
                0 => {
                    *token0_amount = 0;
                    *close_on_depletion = Some(0);
                }
                1 => *virtual_reserve0 = 0,
                2 => *allowed_input_token = Some(2),
                3 => *close_on_depletion = Some(2),
                4 => *owner_program_id = Some(PROGRAM),
                5 => *owner_program_seed = Some([9; 32]),
                6 => *curve_program_id = AccountId::new([8; 32]),
                7 => *close_timestamp = Some(1),
                _ => *token1_definition_id = TOKEN0,
            }
        }
        cases.push(invalid);
    }
    for invalid in cases {
        assert!(
            handle(&call(
                pool_id(),
                ActorState::empty(),
                Some(Actor::new(OWNER, PROGRAM)),
                invalid
            ))
            .is_err()
        );
    }
    assert!(matches!(
        start(&pool_state(), message.clone()),
        Err(crate::Error::AlreadyInitialized)
    ));
    assert!(matches!(
        handle(&call(
            AccountId::new([77; 32]),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            message
        )),
        Err(crate::Error::WrongAccount)
    ));
}

#[test]
fn prepared_pool_cannot_trade_or_settle_until_funding_and_activates_only_once() {
    let message = create_message(true);
    let created = handle(&call(
        pool_id(),
        ActorState::empty(),
        Some(Actor::new(OWNER, PROGRAM)),
        message,
    ))
    .unwrap();
    let response = answer_reads(created, &pool_state(), namespace(), [0, 0]).unwrap();
    let state = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert!(!state.funded);
    assert_eq!(response.sync_messages.len(), 3);
    for message in [
        Message::ClosePool { now: 1 },
        Message::WithdrawReserves { now: 42 },
        Message::CollectFees,
    ] {
        assert!(matches!(
            start(&state, message),
            Err(crate::Error::PendingFunding)
        ));
    }
    assert!(matches!(
        answer_reads(
            start(
                &state,
                Message::SwapExactInput {
                    amount_in: 1,
                    min_amount_out: 0,
                    token_in: TOKEN1,
                    membership: None,
                    now: 1
                }
            )
            .unwrap(),
            &state,
            namespace(),
            [800, 25]
        ),
        Err(crate::Error::PendingFunding)
    ));
    let activated = answer_reads(
        start(&state, Message::ActivatePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [0, 0],
    )
    .unwrap();
    let funded = PoolAccount::decode(&activated.post_state.unwrap()).unwrap();
    assert!(funded.funded);
    assert_eq!(funded.pool.close_timestamp, Some(42));
    assert!(matches!(
        start(&funded, Message::ActivatePool { now: 2 }),
        Err(crate::Error::AlreadyInitialized)
    ));
}

#[test]
fn factory_creation_relay_rejects_policy_for_another_namespace() {
    let factory = AccountId::new([61; 32]);
    let seed = [62; 32];
    let owner = AccountId::for_public_pda(&factory, &lee_core::program::PdaSeed::new(seed));
    let id = compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, owner);
    let mut message = create_message(true);
    if let Message::CreatePool {
        buy_policy,
        owner: stored,
        owner_program_id,
        owner_program_seed,
        ..
    } = &mut message
    {
        *stored = owner;
        *owner_program_id = Some(factory);
        *owner_program_seed = Some(seed);
        *buy_policy = crate::BuyPolicy::Allowlist(crate::AllowlistPolicy {
            version: 1,
            namespace: [99; 32],
            sale_policy_id: *id.value(),
            root: [8; 32],
        });
    }
    let mut request = call(
        owner,
        ActorState::empty(),
        Some(Actor::new(owner, factory)),
        message,
    );
    request.is_authorized = true;
    assert!(matches!(handle(&request), Err(crate::Error::InvalidPool)));
}

#[test]
fn factory_creation_relay_requires_its_owner_program_and_grants_only_the_pool_seed() {
    let factory = AccountId::new([61; 32]);
    let seed = [62; 32];
    let owner = AccountId::for_public_pda(&factory, &lee_core::program::PdaSeed::new(seed));
    let id = compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, owner);
    let mut message = create_message(true);
    if let Message::CreatePool {
        owner: stored,
        owner_program_id,
        owner_program_seed,
        ..
    } = &mut message
    {
        *stored = owner;
        *owner_program_id = Some(factory);
        *owner_program_seed = Some(seed);
    }
    let mut request = call(
        owner,
        ActorState::empty(),
        Some(Actor::new(owner, factory)),
        message.clone(),
    );
    request.is_authorized = true;
    let forwarded = handle(&request).unwrap();
    assert!(forwarded.post_state.is_none());
    assert_eq!(forwarded.sync_messages.len(), 1);
    let forwarded = &forwarded.sync_messages[0];
    assert_eq!(forwarded.to, Actor::new(id, PROGRAM));
    assert_eq!(
        borsh::from_slice::<Message>(&forwarded.message).unwrap(),
        message
    );
    assert_eq!(
        forwarded.pda_seeds,
        [crate::compute_pool_pda_seed(NS, TOKEN0, TOKEN1, owner)].into()
    );
    let pending = handle(&ReceiveInput {
        receiver: forwarded.to,
        pre_state: ActorState::empty(),
        from: Some(request.receiver),
        is_authorized: true,
        message: forwarded.message.clone(),
    })
    .unwrap();
    assert_eq!(
        pending.sync_messages[0].to,
        Actor::new(compute_config_pda(NS, PROGRAM), PROGRAM)
    );
    for sender in [
        Actor::new(NS, PROGRAM),
        Actor::new(owner, PROGRAM),
        Actor::new(OWNER, factory),
    ] {
        let mut forged = request.clone();
        forged.from = Some(sender);
        assert!(matches!(handle(&forged), Err(crate::Error::Unauthorized)));
    }
    request.is_authorized = false;
    assert!(matches!(handle(&request), Err(crate::Error::Unauthorized)));
    request.is_authorized = true;
    if let Message::CreatePool {
        owner_program_seed, ..
    } = &mut message
    {
        *owner_program_seed = Some([63; 32]);
    }
    request.message = borsh::to_vec(&message).unwrap();
    assert!(matches!(handle(&request), Err(crate::Error::Unauthorized)));
}

#[test]
fn pool_creation_limits_inclusion_before_expiry_only_when_funded() {
    for (deferred, now, before) in [
        (false, 0, Some(42_000)),
        (false, 1, Some(42_000)),
        (true, 0, None),
        (true, 42, None),
    ] {
        let mut message = create_message(deferred);
        if let Message::CreatePool { now: supplied, .. } = &mut message {
            *supplied = now;
        }
        let pending = handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            message,
        ))
        .unwrap();
        let response = answer_reads(pending, &pool_state(), namespace(), [0, 0]).unwrap();
        assert_eq!(response.validity.timestamps.start(), None);
        assert_eq!(response.validity.timestamps.end(), before);
    }
}

#[test]
fn activation_binds_open_and_expired_branches_to_inclusion_time() {
    let mut state = pool_state();
    state.funded = false;
    for now in [0, 42] {
        let response = answer_reads(
            start(&state, Message::ActivatePool { now }).unwrap(),
            &state,
            namespace(),
            [0, 0],
        )
        .unwrap();
        let window = response.validity.timestamps;
        if now < 42 {
            assert_eq!((window.start(), window.end()), (None, Some(42_000)));
            assert!(window.is_valid_for(41_999));
            assert!(!window.is_valid_for(42_000));
        } else {
            assert_eq!((window.start(), window.end()), (Some(42_000), None));
            assert!(!window.is_valid_for(41_999));
            assert!(window.is_valid_for(42_000));
        }
        let funded = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
        assert_eq!(funded.pool.close_timestamp, Some(42));
        assert_eq!(
            funded.pool.effective_lifecycle(42),
            pool::PoolLifecycle::Closed
        );
        assert!(
            answer_reads(
                start(
                    &funded,
                    Message::SwapExactInput {
                        amount_in: 28,
                        min_amount_out: 0,
                        token_in: TOKEN1,
                        membership: None,
                        now: 42
                    }
                )
                .unwrap(),
                &funded,
                namespace(),
                [800, 100]
            )
            .is_err()
        );
        if now == 0 {
            let swap = answer_reads(
                start(
                    &funded,
                    Message::SwapExactInput {
                        amount_in: 28,
                        min_amount_out: 0,
                        token_in: TOKEN1,
                        membership: None,
                        now: 1,
                    },
                )
                .unwrap(),
                &funded,
                namespace(),
                [800, 100],
            )
            .unwrap();
            assert_eq!(swap.validity.timestamps.start(), None);
            assert_eq!(swap.validity.timestamps.end(), Some(42_000));
        }
        let withdrawn = answer_reads(
            start(&funded, Message::WithdrawReserves { now: 42 }).unwrap(),
            &funded,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(
            PoolAccount::decode(&withdrawn.post_state.unwrap())
                .unwrap()
                .pool
                .lifecycle,
            pool::PoolLifecycle::Withdrawn
        );
    }
}

#[test]
fn prepared_pool_funding_after_expiry_closes_the_original_deadline() {
    let mut state = pool_state();
    state.funded = false;
    let response = answer_reads(
        start(&state, Message::ActivatePool { now: 42 }).unwrap(),
        &state,
        namespace(),
        [0, 0],
    )
    .unwrap();
    let funded = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert!(funded.funded);
    assert_eq!(funded.pool.lifecycle, pool::PoolLifecycle::Closed);
    assert_eq!(funded.pool.close_timestamp, Some(42));
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .filter(|m| matches!(m, lez_compat_core::token::Message::Transfer { .. }))
        .collect();
    assert_eq!(transfers.len(), 2);
}

#[test]
fn pause_policy_survives_deferred_creation_and_activation() {
    for policy in [
        crate::PausePolicy::Disabled,
        crate::PausePolicy::AuthorityControlled,
    ] {
        let mut message = create_message(true);
        if let Message::CreatePool { pause_policy, .. } = &mut message {
            *pause_policy = policy;
        }
        let created = handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            message,
        ))
        .unwrap();
        let created = answer_reads(created, &pool_state(), namespace(), [0, 0]).unwrap();
        let state = PoolAccount::decode(&created.post_state.unwrap()).unwrap();
        assert_eq!(state.trade_control, crate::TradeControl::new(policy));
        let activated = answer_reads(
            start(&state, Message::ActivatePool { now: 1 }).unwrap(),
            &state,
            namespace(),
            [0, 0],
        )
        .unwrap();
        assert_eq!(
            PoolAccount::decode(&activated.post_state.unwrap())
                .unwrap()
                .trade_control
                .policy(),
            policy
        );
    }
}

#[test]
fn direct_creation_checks_ending_combinations_before_reads() {
    for ending in [
        crate::EndingPolicy::Manual,
        crate::EndingPolicy::Scheduled,
        crate::EndingPolicy::Depletion,
        crate::EndingPolicy::Either,
    ] {
        for end in [None, Some(42)] {
            for side in [None, Some(0), Some(1)] {
                let valid = match ending {
                    crate::EndingPolicy::Manual => end.is_none() && side.is_none(),
                    crate::EndingPolicy::Scheduled => end.is_some() && side.is_none(),
                    crate::EndingPolicy::Depletion => end.is_none() && side.is_some(),
                    crate::EndingPolicy::Either => end.is_some() && side.is_some(),
                };
                let mut message = create_message(true);
                if let Message::CreatePool {
                    ending_policy,
                    close_timestamp,
                    close_on_depletion,
                    ..
                } = &mut message
                {
                    *ending_policy = ending;
                    *close_timestamp = end;
                    *close_on_depletion = side;
                }
                let result = handle(&call(
                    pool_id(),
                    ActorState::empty(),
                    Some(Actor::new(OWNER, PROGRAM)),
                    message,
                ));
                if valid {
                    let created =
                        answer_reads(result.unwrap(), &pool_state(), namespace(), [0, 0]).unwrap();
                    let state = PoolAccount::decode(&created.post_state.unwrap()).unwrap();
                    let activated = answer_reads(
                        start(&state, Message::ActivatePool { now: 1 }).unwrap(),
                        &state,
                        namespace(),
                        [0, 0],
                    )
                    .unwrap();
                    let state = PoolAccount::decode(&activated.post_state.unwrap()).unwrap();
                    assert_eq!(state.pool.ending_policy, ending);
                    assert_eq!(state.pool.close_timestamp, end);
                    assert_eq!(
                        state.pool.close_on_depletion,
                        crate::pool_create::side(side).unwrap()
                    );
                } else {
                    assert!(matches!(result, Err(crate::Error::InvalidPool)));
                }
            }
        }
    }
}

#[test]
fn pending_creation_rejects_policy_terms_that_differ_from_saved_state() {
    let mut message = create_message(true);
    if let Message::CreatePool { ending_policy, .. } = &mut message {
        *ending_policy = crate::EndingPolicy::Either;
    }
    let pending = crate::dispatch::Pending::new(pool_state(), message, HOLDER, vec![]);
    assert!(matches!(
        pending.advance(Actor::new(pool_id(), PROGRAM)),
        Err(crate::Error::InvalidPool)
    ));
}

#[test]
fn creation_rejects_timestamp_conversion_overflow_even_before_deferred_funding() {
    for deferred in [false, true] {
        let mut message = create_message(deferred);
        if let Message::CreatePool {
            close_timestamp, ..
        } = &mut message
        {
            *close_timestamp = Some(u64::MAX / 1000 + 1);
        }
        assert!(matches!(
            handle(&call(
                pool_id(),
                ActorState::empty(),
                Some(Actor::new(OWNER, PROGRAM)),
                message
            )),
            Err(crate::Error::Arithmetic)
        ));
    }
    let mut state = pool_state();
    state.funded = false;
    state.pool.close_timestamp = Some(u64::MAX / 1000 + 1);
    assert!(matches!(
        answer_reads(
            start(&state, Message::ActivatePool { now: 1 }).unwrap(),
            &state,
            namespace(),
            [0, 0]
        ),
        Err(crate::Error::Arithmetic)
    ));
}

#[test]
fn retained_fee_is_immutable_through_direct_creation_and_activation() {
    for rate in [0, 125, 9999] {
        let mut message = create_message(true);
        if let Message::CreatePool { sell_fee_bps, .. } = &mut message {
            *sell_fee_bps = rate;
        }
        let created = handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            message.clone(),
        ))
        .unwrap();
        let created = answer_reads(created, &pool_state(), namespace(), [0, 0]).unwrap();
        let state = PoolAccount::decode(created.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.pool.sell_fee_bps, rate);
        assert_eq!(state.pool.initial_k, 100_000);
        let activated = answer_reads(
            start(&state, Message::ActivatePool { now: 1 }).unwrap(),
            &state,
            namespace(),
            [0, 0],
        )
        .unwrap();
        assert_eq!(
            PoolAccount::decode(activated.post_state.as_ref().unwrap())
                .unwrap()
                .pool
                .sell_fee_bps,
            rate
        );
        let mut forged = state;
        forged.pool.sell_fee_bps = if rate == 0 { 1 } else { 0 };
        let pending = crate::dispatch::Pending::new(forged, message, OWNER, vec![]);
        assert!(matches!(
            pending.advance(Actor::new(pool_id(), PROGRAM)),
            Err(crate::Error::InvalidPool)
        ));
    }
    let mut message = create_message(true);
    if let Message::CreatePool { sell_fee_bps, .. } = &mut message {
        *sell_fee_bps = 10_000;
    }
    assert!(matches!(
        handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(OWNER, PROGRAM)),
            message
        )),
        Err(crate::Error::InvalidPool)
    ));
}
