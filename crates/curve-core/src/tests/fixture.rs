use crate::{
    Message, PoolAccount, TOKEN_PROGRAM_ACCOUNT_ID, compute_config_pda, compute_pool_pda, handle,
};
use lee_core::account::{AccountId, Actor, ActorState};
use lee_core::program::{ReceiveInput, Response, StateReply, SyncMessage};
use lez_compat_core::token::{TokenKind, TokenState};

pub(super) const PROGRAM: AccountId = AccountId::new([7; 32]);

pub(super) const NS: AccountId = AccountId::new([51; 32]);

pub(super) const OWNER: AccountId = AccountId::new([3; 32]);

pub(super) const HOLDER: AccountId = AccountId::new([91; 32]);

pub(super) const TOKEN0: AccountId = AccountId::new([1; 32]);

pub(super) const TOKEN1: AccountId = AccountId::new([2; 32]);

pub(super) fn pool_id() -> AccountId {
    compute_pool_pda(NS, PROGRAM, TOKEN0, TOKEN1, OWNER)
}

pub(super) fn call(
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

pub(super) fn namespace() -> ActorState {
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

pub(super) fn create_message(defer_funding: bool) -> Message {
    Message::CreatePool {
        buy_policy: crate::BuyPolicy::Open,
        pause_policy: crate::PausePolicy::Disabled,
        ending_policy: crate::EndingPolicy::Scheduled,
        sell_fee_bps: 0,
        per_block_token_limit: None,
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

pub(super) fn reply_to(
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

pub(super) fn pool_state() -> PoolAccount {
    PoolAccount {
        buy_policy: crate::BuyPolicy::Open,
        trade_control: crate::TradeControl::new(crate::PausePolicy::Disabled),
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
        block_allocation: Default::default(),
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
            ending_policy: pool::EndingPolicy::Scheduled,
            sell_fee_bps: 0,
            close_timestamp: Some(42),
            close_on_depletion: None,
        })
        .unwrap(),
    }
}

pub(super) fn start(state: &PoolAccount, message: Message) -> Result<Response, crate::Error> {
    handle(&call(
        pool_id(),
        borsh::to_vec(state).unwrap().into(),
        Some(Actor::new(HOLDER, PROGRAM)),
        message,
    ))
}

pub(super) fn answer_reads(
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
        if response.sync_messages[0].to == clock_core::clock_actor() {
            break;
        }
        let query = response.sync_messages.remove(0);
        let (from, bytes) = if query.to == Actor::new(HOLDER, crate::AUTHORITY_PROGRAM_ACCOUNT_ID) {
            let role = crate::compute_pool_owner_role(NS, pool_id(), PROGRAM);
            (
                Actor::new(role, crate::AUTHORITY_PROGRAM_ACCOUNT_ID),
                borsh::to_vec(&lez_compat_core::authority::Authorized {
                    role,
                    authority: HOLDER,
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

pub(super) fn event_kinds(response: &Response) -> Vec<crate::events::SaleEventKindV1> {
    use crate::events::{EventSubjectV1, PricingEngineV1, SaleEventV1};
    use risc0_zkvm::sha::{Impl, Sha256 as _};
    let digest = Impl::hash_bytes(SaleEventV1::SELECTOR_NAME.as_bytes());
    response
        .events
        .iter()
        .map(|event| {
            assert_eq!(event.selector.as_slice(), &digest.as_bytes()[..8]);
            let payload: SaleEventV1 = borsh::from_slice(&event.data).unwrap();
            assert_eq!(payload.version, 1);
            assert_eq!(payload.namespace, *NS.value());
            assert_eq!(payload.engine, PricingEngineV1::Curve);
            assert_eq!(
                payload.subject,
                EventSubjectV1::Pool {
                    pool: *pool_id().value()
                }
            );
            payload.kind
        })
        .collect()
}
