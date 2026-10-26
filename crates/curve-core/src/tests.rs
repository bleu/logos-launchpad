//! Public actor-message tests replace the old whole-account adapter tests.

#[test]
fn native_clock_conversion_checks_overflow_and_keeps_only_unbounded_upper_max() {
    let largest = u64::MAX / 1000;
    assert_eq!(crate::timestamp_millis(largest), Ok(largest * 1000));
    assert_eq!(
        crate::timestamp_millis(largest + 1),
        Err(crate::Error::Arithmetic)
    );
    assert_eq!(
        crate::clock_assertion(0, None),
        Ok(clock_core::Message::AssertTimestamp {
            at_least: 0,
            at_most: u64::MAX
        })
    );
    assert_eq!(
        crate::clock_assertion(u64::MAX, None),
        Err(crate::Error::Arithmetic)
    );
    assert_eq!(
        crate::clock_assertion(0, Some(u64::MAX)),
        Err(crate::Error::Arithmetic)
    );
    assert_eq!(
        crate::clock_assertion(0, Some(0)),
        Err(crate::Error::Arithmetic)
    );
    assert_eq!(
        crate::clock_assertion(42, Some(42)),
        Err(crate::Error::Closed)
    );
}

#[test]
fn closed_pool_withdrawal_does_not_wait_for_its_future_deadline() {
    let mut state = pool_state();
    state.pool.lifecycle = pool::PoolLifecycle::Closed;
    for deadline in [Some(42), None] {
        state.pool.close_timestamp = deadline;
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now: 1 }).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
            clock_core::Message::AssertTimestamp {
                at_least: 1000,
                at_most: u64::MAX
            }
        );
        assert_eq!(
            PoolAccount::decode(&response.post_state.unwrap())
                .unwrap()
                .pool
                .lifecycle,
            pool::PoolLifecycle::Withdrawn
        );
    }
}

#[test]
fn pools_without_expiry_keep_time_independent_trading_and_cannot_withdraw_while_open() {
    let mut state = pool_state();
    state.pool.close_timestamp = None;
    let response = answer_reads(
        start(
            &state,
            Message::SwapExactInput {
                amount_in: 28,
                min_amount_out: 0,
                token_in: TOKEN1,
                now: 1,
            },
        )
        .unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert!(
        response
            .sync_messages
            .iter()
            .all(|message| message.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
    );
    assert!(matches!(
        answer_reads(
            start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
            &state,
            namespace(),
            [800, 100]
        ),
        Err(crate::Error::Closed)
    ));
    let closed = answer_reads(
        start(&state, Message::ClosePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&closed.sync_messages[0].message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 1000,
            at_most: u64::MAX
        }
    );
}
use crate::{
    Config, Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID, compute_config_pda, compute_pool_pda,
    handle,
};
use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::{ReceiveInput, Response, StateReply, SyncMessage},
};
use lez_compat_core::token::{TokenKind, TokenState};
const PROGRAM: AccountId = AccountId::new([7; 32]);
const NS: AccountId = AccountId::new([51; 32]);
const OWNER: AccountId = AccountId::new([3; 32]);
const HOLDER: AccountId = AccountId::new([91; 32]);
const TOKEN0: AccountId = AccountId::new([1; 32]);
const TOKEN1: AccountId = AccountId::new([2; 32]);
fn pool_id() -> AccountId {
    compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, OWNER)
}
fn call(
    receiver: AccountId,
    state: ActorState,
    from: Option<Actor>,
    message: Message,
) -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(receiver, PROGRAM),
        pre_state: state,
        from,
        message: borsh::to_vec(&message).unwrap(),
        is_authorized: false,
    }
}
fn namespace() -> ActorState {
    let mut signed = call(
        NS,
        ActorState::empty(),
        None,
        Message::InitializeNamespace {
            namespace: NS,
            protocol_fee_bps: 100,
            treasury: AccountId::new([6; 32]),
        },
    );
    signed.is_authorized = true;
    let forward = handle(&signed).unwrap().sync_messages.remove(0);
    handle(&ReceiveInput {
        receiver: forward.to,
        from: Some(signed.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: forward.message,
    })
    .unwrap()
    .post_state
    .unwrap()
}
fn create_message(defer_funding: bool) -> Message {
    Message::CreatePool {
        namespace: NS,
        token0_definition_id: TOKEN0,
        token1_definition_id: TOKEN1,
        allowed_input_token: None,
        defer_funding,
        token0_amount: 800,
        token1_amount: 25,
        virtual_reserve0: 1000,
        virtual_reserve1: 100,
        close_timestamp: Some(42),
        close_on_depletion: None,
        owner: OWNER,
        owner_program_id: None,
        owner_program_seed: None,
        curve_program_id: PROGRAM,
        now: 1,
    }
}
fn reply_to(
    state: ActorState,
    query: &SyncMessage,
    message: Vec<u8>,
) -> Result<Response, crate::Error> {
    handle(&ReceiveInput {
        receiver: Actor::new(pool_id(), PROGRAM),
        from: Some(query.to),
        is_authorized: false,
        pre_state: state,
        message,
    })
}
#[test]
fn nft_owner_creates_and_funds_both_ordered_reserves() {
    let mut signed = call(
        HOLDER,
        ActorState::empty(),
        None,
        Message::Execute {
            pool: pool_id(),
            instruction: Box::new(create_message(false)),
        },
    );
    signed.is_authorized = true;
    let forward = handle(&signed).unwrap().sync_messages.remove(0);
    let mut response = handle(&ReceiveInput {
        receiver: forward.to,
        from: Some(signed.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: forward.message,
    })
    .unwrap();
    // Each reply is supplied at the public message boundary, with its authoritative actor origin.
    for _ in 0..4 {
        let query = response.sync_messages.remove(0);
        let message = if query.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID
            && query.to.account_id == HOLDER
        {
            let checked = lez_compat_core::token::handle(&ReceiveInput {
                receiver: query.to,
                from: Some(Actor::new(pool_id(), PROGRAM)),
                is_authorized: true,
                pre_state: ActorState::empty(),
                message: query.message,
            })
            .unwrap()
            .sync_messages
            .remove(0);
            let proof = lez_compat_core::token::handle(&ReceiveInput {
                receiver: checked.to,
                from: Some(query.to),
                is_authorized: false,
                pre_state: borsh::to_vec(&TokenState::Holding {
                    definition: OWNER,
                    owner: HOLDER,
                    kind: TokenKind::NftMaster,
                    balance: 1,
                })
                .unwrap()
                .into(),
                message: checked.message,
            })
            .unwrap()
            .sync_messages
            .remove(0);
            response = handle(&ReceiveInput {
                receiver: proof.to,
                from: Some(checked.to),
                is_authorized: false,
                pre_state: response.post_state.unwrap(),
                message: proof.message,
            })
            .unwrap();
            continue;
        } else if query.to == Actor::new(compute_config_pda(NS, PROGRAM), PROGRAM) {
            borsh::to_vec(&StateReply { state: namespace() }).unwrap()
        } else {
            borsh::to_vec(&StateReply {
                state: borsh::to_vec(&TokenState::Definition {
                    name: "Token".into(),
                    kind: TokenKind::Fungible,
                    supply: 10000,
                    metadata: None,
                })
                .unwrap()
                .into(),
            })
            .unwrap()
        };
        response = reply_to(response.post_state.unwrap(), &query, message).unwrap();
    }
    let pool = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(pool.owner, OWNER);
    assert_eq!(pool.pool.k, 100000);
    assert_eq!(
        (pool.pool.real_reserve0, pool.pool.real_reserve1),
        (800, 25)
    );
    assert!(pool.funded);
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .filter(|m| matches!(m, lez_compat_core::token::Message::Transfer { .. }))
        .collect();
    assert_eq!(
        transfers,
        vec![
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: pool_id(),
                amount: 800
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: pool_id(),
                amount: 25
            }
        ]
    );
    assert!(
        response
            .sync_messages
            .iter()
            .filter(|m| m.to.account_id == HOLDER)
            .all(|m| m.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(Config::decode(&namespace()).unwrap().namespace, NS);
}

#[test]
fn pool_read_replies_with_canonical_state_to_the_requesting_actor() {
    let state = pool_state();
    let response = start(&state, Message::ReadState).unwrap();
    assert!(response.post_state.is_none());
    assert_eq!(response.sync_messages.len(), 1);
    let reply = &response.sync_messages[0];
    assert_eq!(reply.to, Actor::new(HOLDER, PROGRAM));
    assert_eq!(
        borsh::from_slice::<StateReply>(&reply.message)
            .unwrap()
            .state,
        borsh::to_vec(&state).unwrap().into()
    );
    let foreign = call(
        AccountId::new([71; 32]),
        borsh::to_vec(&state).unwrap().into(),
        Some(Actor::new(HOLDER, PROGRAM)),
        Message::ReadState,
    );
    assert!(matches!(handle(&foreign), Err(crate::Error::WrongAccount)));
}

#[test]
fn accrued_fees_cannot_back_swaps_collection_or_withdrawal() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    for message in [
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 0,
            token_in: TOKEN0,
            now: 1,
        },
        Message::CollectFees,
        Message::WithdrawReserves { now: 42 },
    ] {
        assert!(matches!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                namespace(),
                [800, 100]
            ),
            Err(crate::Error::Underfunded)
        ));
    }
    assert!(matches!(
        answer_reads(
            start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
            &state,
            namespace(),
            [799, 107]
        ),
        Err(crate::Error::Underfunded)
    ));
}

#[test]
fn namespace_and_vault_replies_require_the_exact_account_and_program() {
    let state = pool_state();
    let response = start(
        &state,
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 0,
            token_in: TOKEN0,
            now: 1,
        },
    )
    .unwrap();
    let query = &response.sync_messages[0];
    let pending = response.post_state.unwrap();
    let bytes = borsh::to_vec(&StateReply { state: namespace() }).unwrap();
    for origin in [
        Actor::new(query.to.account_id, AccountId::new([77; 32])),
        Actor::new(AccountId::new([78; 32]), query.to.program_account_id),
    ] {
        assert!(matches!(
            handle(&ReceiveInput {
                receiver: Actor::new(pool_id(), PROGRAM),
                from: Some(origin),
                is_authorized: false,
                pre_state: pending.clone(),
                message: bytes.clone()
            }),
            Err(crate::Error::Unauthorized)
        ));
    }
    let mut foreign = Config::decode(&namespace()).unwrap();
    foreign.namespace = AccountId::new([79; 32]);
    assert!(matches!(
        reply_to(
            pending.clone(),
            query,
            borsh::to_vec(&StateReply {
                state: borsh::to_vec(&foreign).unwrap().into()
            })
            .unwrap()
        ),
        Err(crate::Error::WrongAccount)
    ));
    let response = reply_to(pending, query, bytes).unwrap();
    let vault_query = &response.sync_messages[0];
    let malformed = TokenState::Holding {
        definition: TOKEN0,
        owner: HOLDER,
        kind: TokenKind::Fungible,
        balance: 10000,
    };
    assert!(matches!(
        reply_to(
            response.post_state.unwrap(),
            vault_query,
            borsh::to_vec(&StateReply {
                state: borsh::to_vec(&malformed).unwrap().into()
            })
            .unwrap()
        ),
        Err(crate::Error::InvalidToken)
    ));
}

#[test]
fn owner_actions_require_a_signed_current_master_nft_holder() {
    let state = pool_state();
    let response = start(&state, Message::ClosePool { now: 1 }).unwrap();
    let request = response.sync_messages.into_iter().next().unwrap();
    let mut owner_input = ReceiveInput {
        receiver: request.to,
        from: Some(Actor::new(pool_id(), PROGRAM)),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: request.message,
    };
    assert!(lez_compat_core::token::handle(&owner_input).is_err());
    owner_input.is_authorized = true;
    let query = lez_compat_core::token::handle(&owner_input)
        .unwrap()
        .sync_messages
        .remove(0);
    for (definition, owner, kind, balance) in [
        (OWNER, HOLDER, TokenKind::NftMaster, 0),
        (OWNER, HOLDER, TokenKind::Fungible, 1),
        (OWNER, AccountId::new([90; 32]), TokenKind::NftMaster, 1),
        (TOKEN0, HOLDER, TokenKind::NftMaster, 1),
    ] {
        let holding = TokenState::Holding {
            definition,
            owner,
            kind,
            balance,
        };
        assert!(
            lez_compat_core::token::handle(&ReceiveInput {
                receiver: query.to,
                from: Some(owner_input.receiver),
                is_authorized: false,
                pre_state: borsh::to_vec(&holding).unwrap().into(),
                message: query.message.clone()
            })
            .is_err()
        );
    }
    let proof = borsh::to_vec(&lez_compat_core::token::NftHolder {
        definition: OWNER,
        owner: HOLDER,
    })
    .unwrap();
    assert!(matches!(
        handle(&ReceiveInput {
            receiver: Actor::new(pool_id(), PROGRAM),
            from: Some(Actor::new(query.to.account_id, PROGRAM)),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: proof
        }),
        Err(crate::Error::Unauthorized)
    ));
}

#[test]
fn namespace_freeze_blocks_each_swap_and_unfreeze_does_not_reopen_expired_pools() {
    let state = pool_state();
    let mut config = Config::decode(&namespace()).unwrap();
    config.emergency.is_frozen = true;
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 1,
            token_in: TOKEN1,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 100,
            token_in: TOKEN1,
            now: 1,
        },
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 1,
            token_in: TOKEN0,
            now: 1,
        },
    ] {
        assert!(matches!(
            answer_reads(
                start(&state, message.clone()).unwrap(),
                &state,
                borsh::to_vec(&config).unwrap().into(),
                [800, 100]
            ),
            Err(crate::Error::Frozen)
        ));
        assert!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                namespace(),
                [800, 100]
            )
            .is_ok()
        );
    }
    for lifecycle in [
        pool::PoolLifecycle::Open,
        pool::PoolLifecycle::Closed,
        pool::PoolLifecycle::Withdrawn,
    ] {
        let mut retired = state.clone();
        retired.pool.lifecycle = lifecycle;
        assert!(
            answer_reads(
                start(
                    &retired,
                    Message::SwapExactInput {
                        amount_in: 28,
                        min_amount_out: 0,
                        token_in: TOKEN1,
                        now: 42
                    }
                )
                .unwrap(),
                &retired,
                namespace(),
                [800, 100]
            )
            .is_err()
        );
    }
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
        Some(Actor::new(HOLDER, PROGRAM)),
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
                Some(Actor::new(HOLDER, PROGRAM)),
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
            Some(Actor::new(HOLDER, PROGRAM)),
            message
        )),
        Err(crate::Error::WrongAccount)
    ));
}

#[test]
fn neutral_input_restrictions_apply_to_both_buy_forms_and_sells() {
    let mut state = pool_state();
    state.allowed_input_token = Some(pool::TokenSide::Token0);
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 0,
            token_in: TOKEN1,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 100,
            token_in: TOKEN1,
            now: 1,
        },
    ] {
        assert!(matches!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                namespace(),
                [800, 100]
            ),
            Err(crate::Error::Restricted)
        ));
    }
    state.allowed_input_token = Some(pool::TokenSide::Token1);
    assert!(matches!(
        answer_reads(
            start(
                &state,
                Message::SwapExactInput {
                    amount_in: 250,
                    min_amount_out: 0,
                    token_in: TOKEN0,
                    now: 1
                }
            )
            .unwrap(),
            &state,
            namespace(),
            [800, 100]
        ),
        Err(crate::Error::Restricted)
    ));
    state.allowed_input_token = None;
    assert!(
        answer_reads(
            start(
                &state,
                Message::SwapExactOutput {
                    amount_out: 19,
                    max_amount_in: 1000,
                    token_in: TOKEN0,
                    now: 1
                }
            )
            .unwrap(),
            &state,
            namespace(),
            [800, 100]
        )
        .is_err()
    );
}

#[test]
fn live_fee_changes_respect_buy_and_sell_slippage() {
    let state = pool_state();
    let mut high = Config::decode(&namespace()).unwrap();
    high.protocol_fee_bps = 1000;
    for message in [
        Message::SwapExactInput {
            amount_in: 250,
            min_amount_out: 19,
            token_in: TOKEN0,
            now: 1,
        },
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 205,
            token_in: TOKEN1,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 26,
            token_in: TOKEN1,
            now: 1,
        },
    ] {
        assert!(
            answer_reads(
                start(&state, message.clone()).unwrap(),
                &state,
                namespace(),
                [800, 100]
            )
            .is_ok()
        );
        assert!(
            answer_reads(
                start(&state, message).unwrap(),
                &state,
                borsh::to_vec(&high).unwrap().into(),
                [800, 100]
            )
            .is_err()
        );
    }
}

#[test]
fn pool_treasury_leaves_collected_fees_in_custody_and_owner_treasury_receives_both() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    for treasury in [pool_id(), HOLDER] {
        let mut config = Config::decode(&namespace()).unwrap();
        config.treasury = treasury;
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
            &state,
            borsh::to_vec(&config).unwrap().into(),
            [811, 118],
        )
        .unwrap();
        let withdrawn = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
        assert_eq!(
            (withdrawn.pool.fees_accrued, withdrawn.pool.fees_collected),
            (0, 7)
        );
        let transfers: Vec<_> = response
            .sync_messages
            .iter()
            .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
            .collect();
        let expected = if treasury == pool_id() {
            vec![
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN0,
                    recipient: HOLDER,
                    amount: 800,
                },
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN1,
                    recipient: HOLDER,
                    amount: 100,
                },
            ]
        } else {
            vec![
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN1,
                    recipient: HOLDER,
                    amount: 7,
                },
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN0,
                    recipient: HOLDER,
                    amount: 800,
                },
                lez_compat_core::token::Message::Transfer {
                    definition: TOKEN1,
                    recipient: HOLDER,
                    amount: 100,
                },
            ]
        };
        assert_eq!(transfers, expected);
    }
}

#[test]
fn prepared_pool_cannot_trade_or_settle_until_funding_and_activates_only_once() {
    let message = create_message(true);
    let created = handle(&call(
        pool_id(),
        ActorState::empty(),
        Some(Actor::new(HOLDER, PROGRAM)),
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
fn factory_pool_close_requires_the_exact_derived_owner_actor() {
    let owner_program = AccountId::new([61; 32]);
    let seed = [62; 32];
    let owner = AccountId::for_public_pda(&owner_program, &lee_core::program::PdaSeed::new(seed));
    let mut state = pool_state();
    state.owner = owner;
    state.owner_program = Some((owner_program, seed));
    let id = compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, owner);
    let request = call(
        id,
        borsh::to_vec(&state).unwrap().into(),
        Some(Actor::new(owner, owner_program)),
        Message::ClosePool { now: 1 },
    );
    let response = handle(&request).unwrap();
    assert_eq!(
        PoolAccount::decode(&response.post_state.unwrap())
            .unwrap()
            .pool
            .lifecycle,
        pool::PoolLifecycle::Closed
    );
    assert_eq!(response.sync_messages.len(), 1);
    for sender in [
        Actor::new(owner, PROGRAM),
        Actor::new(HOLDER, owner_program),
    ] {
        let mut wrong = request.clone();
        wrong.from = Some(sender);
        assert!(matches!(handle(&wrong), Err(crate::Error::Unauthorized)));
    }
    state.owner_program = Some((owner_program, [63; 32]));
    let mut wrong = request;
    wrong.pre_state = borsh::to_vec(&state).unwrap().into();
    assert!(matches!(handle(&wrong), Err(crate::Error::Unauthorized)));
}

#[test]
fn signed_pool_entry_cannot_be_used_without_authorization_or_for_namespace_control() {
    for instruction in [
        create_message(false),
        Message::ActivatePool { now: 1 },
        Message::ClosePool { now: 1 },
        Message::WithdrawReserves { now: 42 },
        Message::SwapExactInput {
            amount_in: 1,
            min_amount_out: 0,
            token_in: TOKEN1,
            now: 1,
        },
        Message::SwapExactOutput {
            amount_out: 1,
            max_amount_in: 1,
            token_in: TOKEN1,
            now: 1,
        },
    ] {
        let mut request = call(
            HOLDER,
            ActorState::empty(),
            None,
            Message::Execute {
                pool: pool_id(),
                instruction: Box::new(instruction.clone()),
            },
        );
        assert!(matches!(handle(&request), Err(crate::Error::Unauthorized)));
        request.is_authorized = true;
        let forward = handle(&request).unwrap().sync_messages.remove(0);
        assert_eq!(forward.to, Actor::new(pool_id(), PROGRAM));
        assert_eq!(
            borsh::from_slice::<Message>(&forward.message).unwrap(),
            instruction
        );
    }
    let mut request = call(
        HOLDER,
        ActorState::empty(),
        None,
        Message::Execute {
            pool: pool_id(),
            instruction: Box::new(Message::SetFrozen {
                namespace: NS,
                frozen: true,
            }),
        },
    );
    request.is_authorized = true;
    assert!(matches!(handle(&request), Err(crate::Error::Unauthorized)));
}

#[test]
fn swaps_allow_submission_delay_but_require_native_time_before_expiry() {
    let mut state = pool_state();
    state.pool.close_timestamp = Some(1_700_000_042);
    for message in [
        Message::SwapExactInput {
            amount_in: 28,
            min_amount_out: 0,
            token_in: TOKEN1,
            now: 1_700_000_001,
        },
        Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 100,
            token_in: TOKEN1,
            now: 1_700_000_001,
        },
    ] {
        let response = answer_reads(
            start(&state, message).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
            clock_core::Message::AssertTimestamp {
                at_least: 1_700_000_001_000,
                at_most: 1_700_000_041_999
            }
        );
        assert_eq!(
            PoolAccount::decode(&response.post_state.unwrap())
                .unwrap()
                .pool
                .close_timestamp,
            state.pool.close_timestamp
        );
    }
}

#[test]
fn withdrawal_uses_expiry_as_native_lower_bound_without_client_freshness() {
    let state = pool_state();
    for now in [0, 42, 43] {
        let response = answer_reads(
            start(&state, Message::WithdrawReserves { now }).unwrap(),
            &state,
            namespace(),
            [800, 100],
        )
        .unwrap();
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
            clock_core::Message::AssertTimestamp {
                at_least: now.max(42) * 1000,
                at_most: u64::MAX
            }
        );
        assert_eq!(
            PoolAccount::decode(&response.post_state.unwrap())
                .unwrap()
                .pool
                .lifecycle,
            pool::PoolLifecycle::Withdrawn
        );
    }
}

#[test]
fn pool_creation_treats_setup_time_as_a_lower_bound_and_checks_live_expiry() {
    for (deferred, now, at_most) in [
        (false, 0, 41_999),
        (false, 1, 41_999),
        (true, 0, u64::MAX),
        (true, 42, u64::MAX),
    ] {
        let mut message = create_message(deferred);
        if let Message::CreatePool { now: supplied, .. } = &mut message {
            *supplied = now;
        }
        let pending = handle(&call(
            pool_id(),
            ActorState::empty(),
            Some(Actor::new(HOLDER, PROGRAM)),
            message,
        ))
        .unwrap();
        let response = answer_reads(pending, &pool_state(), namespace(), [0, 0]).unwrap();
        let assertion = &response.sync_messages[0];
        assert_eq!(
            assertion.to,
            Actor::new(
                clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                clock_core::clock_account_id()
            )
        );
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&assertion.message).unwrap(),
            clock_core::Message::AssertTimestamp {
                at_least: now * 1000,
                at_most
            }
        );
    }
}

#[test]
fn activation_preserves_zero_lower_bound_without_moving_the_expiry_deadline() {
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
        let assertion = &response.sync_messages[0];
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&assertion.message).unwrap(),
            clock_core::Message::AssertTimestamp {
                at_least: now * 1000,
                at_most: u64::MAX
            }
        );
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
                        now: 1,
                    },
                )
                .unwrap(),
                &funded,
                namespace(),
                [800, 100],
            )
            .unwrap();
            assert_eq!(
                borsh::from_slice::<clock_core::Message>(&swap.sync_messages[0].message).unwrap(),
                clock_core::Message::AssertTimestamp {
                    at_least: 1000,
                    at_most: 41_999
                }
            );
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

fn pool_state() -> PoolAccount {
    PoolAccount {
        allowed_input_token: None,
        funded: true,
        namespace: NS,
        token0_definition_id: TOKEN0,
        token1_definition_id: TOKEN1,
        owner: OWNER,
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .unwrap(),
    }
}
fn start(state: &PoolAccount, message: Message) -> Result<Response, crate::Error> {
    handle(&call(
        pool_id(),
        borsh::to_vec(state).unwrap().into(),
        Some(Actor::new(HOLDER, PROGRAM)),
        message,
    ))
}
fn answer_reads(
    mut response: Response,
    state: &PoolAccount,
    config: ActorState,
    balances: [u128; 2],
) -> Result<Response, crate::Error> {
    while response
        .post_state
        .as_ref()
        .is_some_and(|s| s.starts_with(b"LZCVPEND"))
    {
        let query = response.sync_messages.remove(0);
        let (from, bytes) = if query.to == Actor::new(HOLDER, TOKEN_PROGRAM_ACCOUNT_ID) {
            (
                Actor::new(
                    lez_compat_core::token::holding_address(
                        TOKEN_PROGRAM_ACCOUNT_ID,
                        HOLDER,
                        OWNER,
                    ),
                    TOKEN_PROGRAM_ACCOUNT_ID,
                ),
                borsh::to_vec(&lez_compat_core::token::NftHolder {
                    definition: OWNER,
                    owner: HOLDER,
                })
                .unwrap(),
            )
        } else if query.to == Actor::new(compute_config_pda(NS, PROGRAM), PROGRAM) {
            (
                query.to,
                borsh::to_vec(&StateReply {
                    state: config.clone(),
                })
                .unwrap(),
            )
        } else if query.to.account_id == TOKEN0 || query.to.account_id == TOKEN1 {
            (
                query.to,
                borsh::to_vec(&StateReply {
                    state: borsh::to_vec(&TokenState::Definition {
                        name: "Token".into(),
                        kind: TokenKind::Fungible,
                        supply: 10000,
                        metadata: None,
                    })
                    .unwrap()
                    .into(),
                })
                .unwrap(),
            )
        } else {
            let side = usize::from(
                query.to.account_id
                    == lez_compat_core::token::holding_address(
                        TOKEN_PROGRAM_ACCOUNT_ID,
                        pool_id(),
                        TOKEN1,
                    ),
            );
            let token = TokenState::Holding {
                definition: if side == 0 { TOKEN0 } else { TOKEN1 },
                owner: pool_id(),
                kind: TokenKind::Fungible,
                balance: balances[side],
            };
            (
                query.to,
                borsh::to_vec(&StateReply {
                    state: borsh::to_vec(&token).unwrap().into(),
                })
                .unwrap(),
            )
        };
        response = handle(&ReceiveInput {
            receiver: Actor::new(pool_id(), PROGRAM),
            from: Some(from),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: bytes,
        })?;
    }
    let _ = state;
    Ok(response)
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
fn exact_input_sell_uses_live_collateral_fee_and_synchronous_token_transfers() {
    let state = pool_state();
    let response = answer_reads(
        start(
            &state,
            Message::SwapExactInput {
                amount_in: 250,
                min_amount_out: 19,
                token_in: TOKEN0,
                now: 1,
            },
        )
        .unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    let updated = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            updated.pool.real_reserve0,
            updated.pool.real_reserve1,
            updated.pool.fees_accrued
        ),
        (1050, 80, 1)
    );
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .collect();
    assert_eq!(
        transfers,
        vec![
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: pool_id(),
                amount: 250
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: HOLDER,
                amount: 19
            }
        ]
    );
    assert!(
        response
            .sync_messages
            .last()
            .unwrap()
            .pda_seeds
            .contains(&crate::compute_pool_pda_seed(NS, TOKEN0, TOKEN1, OWNER))
    );
    let clock = &response.sync_messages[0];
    assert_eq!(clock.to.account_id, clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID);
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 1000,
            at_most: 41_999
        }
    );
}

#[test]
fn exact_output_buy_caps_full_collateral_input_including_live_fees() {
    let state = pool_state();
    let mut config = Config::decode(&namespace()).unwrap();
    config.protocol_fee_bps = 1000;
    let message = Message::SwapExactOutput {
        amount_out: 200,
        max_amount_in: 28,
        token_in: TOKEN1,
        now: 1,
    };
    let response = answer_reads(
        start(&state, message).unwrap(),
        &state,
        borsh::to_vec(&config).unwrap().into(),
        [800, 100],
    )
    .unwrap();
    let updated = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            updated.pool.real_reserve0,
            updated.pool.real_reserve1,
            updated.pool.fees_accrued
        ),
        (600, 125, 3)
    );
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .collect();
    assert_eq!(
        transfers,
        vec![
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: pool_id(),
                amount: 28
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: HOLDER,
                amount: 200
            }
        ]
    );
}

#[test]
fn permissionless_collection_uses_current_treasury_and_leaves_donations_as_surplus() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    let mut config = Config::decode(&namespace()).unwrap();
    config.treasury = AccountId::new([64; 32]);
    config.emergency.is_frozen = true;
    let response = answer_reads(
        handle(&call(
            pool_id(),
            borsh::to_vec(&state).unwrap().into(),
            None,
            Message::CollectFees,
        ))
        .unwrap(),
        &state,
        borsh::to_vec(&config).unwrap().into(),
        [800, 118],
    )
    .unwrap();
    let collected = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            collected.pool.fees_accrued,
            collected.pool.fees_collected,
            collected.pool.real_reserve1
        ),
        (0, 7, 100)
    );
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        borsh::from_slice::<lez_compat_core::token::Message>(&response.sync_messages[0].message)
            .unwrap(),
        lez_compat_core::token::Message::Transfer {
            definition: TOKEN1,
            recipient: config.treasury,
            amount: 7
        }
    );
    let replay = answer_reads(
        handle(&call(
            pool_id(),
            borsh::to_vec(&collected).unwrap().into(),
            None,
            Message::CollectFees,
        ))
        .unwrap(),
        &collected,
        borsh::to_vec(&config).unwrap().into(),
        [800, 111],
    )
    .unwrap();
    assert!(replay.sync_messages.is_empty());
}

#[test]
fn expired_withdrawal_pays_current_fees_and_principal_once_leaving_surplus() {
    let mut state = pool_state();
    state.pool.fees_accrued = 7;
    let mut config = Config::decode(&namespace()).unwrap();
    config.emergency.is_frozen = true;
    let response = answer_reads(
        start(&state, Message::WithdrawReserves { now: 42 }).unwrap(),
        &state,
        borsh::to_vec(&config).unwrap().into(),
        [811, 118],
    )
    .unwrap();
    let withdrawn = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(
        (
            withdrawn.pool.real_reserve0,
            withdrawn.pool.real_reserve1,
            withdrawn.pool.fees_accrued,
            withdrawn.pool.fees_collected
        ),
        (0, 0, 0, 7)
    );
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|c| borsh::from_slice::<lez_compat_core::token::Message>(&c.message).ok())
        .collect();
    assert_eq!(
        transfers,
        vec![
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: config.treasury,
                amount: 7
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN0,
                recipient: HOLDER,
                amount: 800
            },
            lez_compat_core::token::Message::Transfer {
                definition: TOKEN1,
                recipient: HOLDER,
                amount: 100
            }
        ]
    );
    assert!(
        answer_reads(
            start(&withdrawn, Message::WithdrawReserves { now: 43 }).unwrap(),
            &withdrawn,
            namespace(),
            [11, 11]
        )
        .is_err()
    );
}

#[test]
fn pool_close_rights_follow_the_current_nft_holder() {
    let state = pool_state();
    let response = answer_reads(
        start(&state, Message::ClosePool { now: 1 }).unwrap(),
        &state,
        namespace(),
        [800, 100],
    )
    .unwrap();
    let closed = PoolAccount::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(closed.pool.lifecycle, pool::PoolLifecycle::Closed);
    assert_eq!(closed.owner, OWNER);
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&response.sync_messages[0].message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 1000,
            at_most: 41_999
        }
    );
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to.account_id,
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID
    );
}
