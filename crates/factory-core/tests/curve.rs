use factory_core::events::*;
use factory_core::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::{
    authority,
    token::{self, TokenKind, TokenState},
};
fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn creation() -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(compute_factory_pda(id(40), id(7), [3; 32]), id(7)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::CreateFactoryPool {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: factory_core::EndingPolicy::Either,
            trade_mode: 0,
            sell_fee_bps: 0,
            per_block_token_limit: None,
            graduation_hook_program_id: None,
            graduation_hook_configuration: vec![],
            namespace: id(40),
            launch_salt: [3; 32],
            name: "Launch".into(),
            uri: "uri".into(),
            graduation_reserve: 0,
            sale_reserve: 800,
            graduation_policy: GraduationPolicy {
                launch_bps: 0,
                collateral_bps: 0,
            },
            creator_allocation: 100,
            virtual_token_reserve: 1000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(400),
            curve_program_id: id(6),
            collateral_definition_id: id(41),
            creator: id(42),
            now: 100,
        })
        .unwrap(),
    }
}
#[test]
fn curve_creation_rejects_policy_for_another_pool() {
    let mut input = creation();
    let mut message: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool { buy_policy, .. } = &mut message {
        *buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: *id(40).value(),
            sale_policy_id: [99; 32],
            root: [8; 32],
        });
    }
    input.message = borsh::to_vec(&message).unwrap();
    assert!(matches!(handle(&input), Err(Error::InvalidTerms)));
}

fn reply(input: &ReceiveInput, response: &Response, bytes: Vec<u8>) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&StateReply {
            state: bytes.into(),
        })
        .unwrap(),
    }
}
fn config() -> Vec<u8> {
    curve_core::handle(&ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace: id(40),
            protocol_fee_bps: 100,
            treasury: id(44),
        })
        .unwrap(),
    })
    .unwrap()
    .post_state
    .unwrap()
    .to_vec()
}
fn definition() -> Vec<u8> {
    borsh::to_vec(&TokenState::Definition {
        name: "Collateral".into(),
        kind: TokenKind::Fungible,
        supply: 1000000,
        metadata: None,
    })
    .unwrap()
}
fn proof(input: &ReceiveInput, response: &Response, creator: AccountId) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&authority::Authorized {
            role: compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            authority: creator,
        })
        .unwrap(),
    }
}
fn resolved(input: &ReceiveInput, response: &Response, creator: AccountId) -> ReceiveInput {
    let role = compute_curve_creator_role_pda(id(40), id(7), [3; 32]);
    let call = response.sync_messages.last().unwrap();
    assert_eq!(call.to, Actor::new(role, AUTHORITY_PROGRAM_ACCOUNT_ID));
    assert_eq!(
        borsh::from_slice::<authority::Message>(&call.message).unwrap(),
        authority::Message::Resolve
    );
    ReceiveInput {
        receiver: input.receiver,
        from: Some(call.to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&authority::CurrentAuthority {
            role,
            authority: creator,
        })
        .unwrap(),
    }
}
fn minted() -> (ReceiveInput, Response) {
    minted_with_policy(curve_core::BuyPolicy::Open)
}
fn minted_with_policy(policy: curve_core::BuyPolicy) -> (ReceiveInput, Response) {
    minted_with_terms(policy, false)
}
fn minted_with_terms(policy: curve_core::BuyPolicy, hook: bool) -> (ReceiveInput, Response) {
    let mut input = creation();
    let mut message: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool {
        buy_policy,
        graduation_hook_program_id,
        graduation_hook_configuration,
        graduation_policy,
        ..
    } = &mut message
    {
        *buy_policy = policy;
        if hook {
            *graduation_hook_program_id = Some(id(9));
            *graduation_hook_configuration = b"route".to_vec();
            *graduation_policy = GraduationPolicy {
                launch_bps: 2500,
                collateral_bps: 3333,
            };
        }
    }
    input.message = borsh::to_vec(&message).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [config(), definition()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}
fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&TokenState::Holding {
        owner,
        definition,
        kind: TokenKind::Fungible,
        balance,
    })
    .unwrap()
}
fn launch_definition(state: &FactoryState) -> Vec<u8> {
    borsh::to_vec(&TokenState::Definition {
        name: state.name.clone(),
        kind: TokenKind::Fungible,
        supply: state.total_supply,
        metadata: Some(token::Metadata {
            standard: token::MetadataStandard::Simple,
            uri: state.uri.clone(),
            creators: String::new(),
        }),
    })
    .unwrap()
}
fn continued(
    input: &ReceiveInput,
    previous: &Response,
    source: u128,
    inventory: u128,
    escrow: u128,
) -> (ReceiveInput, Response) {
    let state = FactoryState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::ContinueCreation {
        creator: id(42),
        now: 100,
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    response = handle(&input).unwrap();
    for bytes in [
        launch_definition(&state),
        holding(
            compute_mint_pda(id(40), id(7), [3; 32]),
            state.token_definition_id,
            source,
        ),
        holding(
            input.receiver.account_id,
            state.token_definition_id,
            inventory,
        ),
        holding(state.creator_escrow_id, state.token_definition_id, escrow),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}
#[test]
fn curve_allocation_funds_only_sale_inventory_and_locks_creator_tokens() {
    let (input, mint) = minted();
    let (input, response) = continued(&input, &mint, 900, 0, 0);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::Allocated);
    let transfers: Vec<_> = response
        .sync_messages
        .iter()
        .filter_map(|m| borsh::from_slice::<token::Message>(&m.message).ok())
        .filter(|m| matches!(m, token::Message::Transfer { .. }))
        .collect();
    assert_eq!(
        transfers,
        vec![
            token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: input.receiver.account_id,
                amount: 800
            },
            token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: state.creator_escrow_id,
                amount: 100
            }
        ]
    );
}
#[test]
fn curve_preparation_uses_neutral_restriction_and_defers_funding() {
    let (input, mint) = minted();
    let (input, allocated) = continued(&input, &mint, 900, 0, 0);
    let (input, response) = continued(&input, &allocated, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::PoolPrepared);
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(input.receiver.account_id, id(6)))
        .unwrap();
    assert_eq!(
        call.pda_seeds,
        [compute_factory_seed(state.namespace, state.launch_salt)].into()
    );
    let curve_core::Message::CreatePool {
        allowed_input_token,
        defer_funding,
        token0_amount,
        token1_amount,
        owner_program_id,
        close_on_depletion,
        ending_policy,
        ..
    } = borsh::from_slice(&call.message).unwrap()
    else {
        panic!("prepare")
    };
    assert_eq!(allowed_input_token, Some(1));
    assert!(defer_funding);
    assert_eq!((token0_amount, token1_amount), (800, 0));
    assert_eq!(owner_program_id, Some(id(7)));
    assert_eq!(close_on_depletion, Some(0));
    assert_eq!(ending_policy, EndingPolicy::Either);
}
fn pool_state(state: &FactoryState, owner: Actor, funded: bool) -> curve_core::PoolAccount {
    curve_core::PoolAccount {
        buy_policy: state.buy_policy.clone(),
        trade_control: curve_core::TradeControl::new(state.pause_policy),
        allowed_input_token: match state.trade_mode {
            TradeMode::BuyOnly => Some(pool::TokenSide::Token1),
            TradeMode::BuyAndSell => None,
        },
        funded,
        closure_observed: false,
        block_allocation: curve_core::BlockAllocation::new(state.per_block_token_limit).unwrap(),
        namespace: state.namespace,
        token0_definition_id: state.token_definition_id,
        token1_definition_id: state.collateral_definition_id,
        owner: owner.account_id,
        owner_program: Some((
            owner.program_account_id,
            *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
        )),
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 1000,
            virtual_reserve1: 100,
            ending_policy: state.ending_policy,
            sell_fee_bps: state.sell_fee_bps,
            close_timestamp: state.end_timestamp,
            close_on_depletion: match state.ending_policy {
                EndingPolicy::Scheduled => None,
                _ => Some(pool::TokenSide::Token0),
            },
        })
        .unwrap(),
    }
}
fn allowlist_policy() -> curve_core::BuyPolicy {
    let factory = compute_factory_pda(id(40), id(7), [3; 32]);
    let pool = curve_core::compute_pool_pda(
        id(40),
        id(6),
        compute_definition_pda(id(40), id(7), [3; 32]),
        id(41),
        factory,
    );
    curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *id(40).value(),
        sale_policy_id: *pool.value(),
        root: [8; 32],
    })
}

#[test]
fn curve_activation_rejects_a_replaced_or_downgraded_pool_policy() {
    let policy = allowlist_policy();
    let (input, mint) = minted_with_policy(policy.clone());
    let (input, allocated) = continued(&input, &mint, 900, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
    let (input, response) = continued(&input, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let curve_core::BuyPolicy::Allowlist(mut replacement) = policy else {
        unreachable!()
    };
    replacement.root = [9; 32];
    for policy in [
        curve_core::BuyPolicy::Open,
        curve_core::BuyPolicy::Allowlist(replacement),
    ] {
        let mut saved = pool_state(&state, input.receiver, false);
        saved.buy_policy = policy;
        let next = reply(&input, &response, borsh::to_vec(&saved).unwrap());
        let next_response = handle(&next).unwrap();
        let next = reply(
            &next,
            &next_response,
            holding(state.pool_id, state.token_definition_id, 0),
        );
        assert!(matches!(handle(&next), Err(Error::InvalidState)));
    }
}

#[test]
fn curve_allowlist_policy_is_unchanged_through_all_creation_stages() {
    let policy = allowlist_policy();
    let (input, mint) = minted_with_policy(policy.clone());
    assert_eq!(
        FactoryState::decode(mint.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let (input, allocated) = continued(&input, &mint, 900, 0, 0);
    assert_eq!(
        FactoryState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
    assert_eq!(
        FactoryState::decode(prepared.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let create = prepared
        .sync_messages
        .iter()
        .find_map(|call| borsh::from_slice::<curve_core::Message>(&call.message).ok())
        .unwrap();
    assert!(
        matches!(create, curve_core::Message::CreatePool { buy_policy, .. } if buy_policy == policy)
    );
    let (mut input, mut response) = continued(&input, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for bytes in [
        borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
        holding(state.pool_id, state.token_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    let active = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(active.creation_stage, CreationStage::Active);
    assert_eq!(active.buy_policy, policy);
    assert_eq!(active.ending_policy, EndingPolicy::Either);
}

fn active() -> (ReceiveInput, Response) {
    active_for_route(false)
}
fn active_for_route(hook: bool) -> (ReceiveInput, Response) {
    let (input, mint) = minted_with_terms(curve_core::BuyPolicy::Open, hook);
    let (input, allocated) = continued(&input, &mint, 900, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
    let (mut input, mut response) = continued(&input, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for bytes in [
        borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
        holding(state.pool_id, state.token_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}
#[test]
fn curve_activation_grants_funding_only_to_the_factory_token_call() {
    let (input, response) = active();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::Active);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(input.receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(
        borsh::from_slice::<token::Message>(&response.sync_messages[0].message).unwrap(),
        token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: state.pool_id,
            amount: 800
        }
    );
    assert_eq!(
        response.sync_messages[1].to,
        Actor::new(state.pool_id, id(6))
    );
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&response.sync_messages[1].message).unwrap(),
        curve_core::Message::ActivatePool { now: 100 }
    );
}
fn settlement(
    input: &ReceiveInput,
    previous: &Response,
    pool: curve_core::PoolAccount,
    tokens: u128,
    collateral: u128,
    hook: bool,
) -> (ReceiveInput, Response) {
    settlement_as(input, previous, pool, tokens, collateral, hook, id(42))
}
fn settlement_as(
    input: &ReceiveInput,
    previous: &Response,
    pool: curve_core::PoolAccount,
    tokens: u128,
    collateral: u128,
    hook: bool,
    creator: AccountId,
) -> (ReceiveInput, Response) {
    let state = FactoryState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&if hook {
        Message::ExecuteGraduationHook {
            creator_hint: Some(creator),
        }
    } else {
        Message::WithdrawFactoryProceeds { creator, now: 400 }
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    input = if hook {
        resolved(&input, &response, creator)
    } else {
        proof(&input, &response, creator)
    };
    response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&pool).unwrap(),
        holding(input.receiver.account_id, state.token_definition_id, tokens),
        holding(
            input.receiver.account_id,
            state.collateral_definition_id,
            collateral,
        ),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    (input, response)
}
#[test]
fn settlement_and_graduation_use_expiry_only_for_a_saved_open_pool() {
    for hook in [false, true] {
        for (lifecycle, expected) in [
            (pool::PoolLifecycle::Open, Some(400_000)),
            (pool::PoolLifecycle::Closed, None),
        ] {
            let (input, active) = active_for_route(hook);
            let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
            let mut saved = pool_state(&state, input.receiver, true);
            saved.pool.lifecycle = lifecycle;
            let (_, response) = settlement(&input, &active, saved, 100, 0, hook);
            assert_eq!(response.validity.timestamps.start(), expected);
            assert_eq!(response.validity.timestamps.end(), None);
        }
    }
}

#[test]
fn expiry_starts_manual_settlement_with_canonical_collateral_custody() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let (input, response) = settlement(
        &input,
        &active,
        pool_state(&state, input.receiver, true),
        100,
        0,
        false,
    );
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.settlement_stage, SettlementStage::Ready);
    assert!(
        matches!(&launch_events(&response)[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Prepared, route: None, effects }, .. }] if effects.is_empty())
    );
    assert_eq!(response.validity.timestamps.start(), Some(400_000));
    assert_eq!(response.validity.timestamps.end(), None);
    assert_eq!(state.pool_id, compute_pool_address(&state, input.receiver));
}
fn compute_pool_address(state: &FactoryState, actor: Actor) -> AccountId {
    curve_core::compute_pool_pda(
        state.namespace,
        state.curve_program_id,
        state.token_definition_id,
        state.collateral_definition_id,
        actor.account_id,
    )
}
fn withdrawn_for_route(
    hook: bool,
    collateral: u128,
) -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, active) = active_for_route(hook);
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, true);
    saved.pool.real_reserve1 = collateral;
    let (input, ready) = settlement(&input, &active, saved.clone(), 17, 19, hook);
    let (input, response) = settlement(&input, &ready, saved.clone(), 17, 19, hook);
    (input, response, saved)
}
fn withdrawn() -> (ReceiveInput, Response, curve_core::PoolAccount) {
    withdrawn_for_route(false, 0)
}
fn payout_ready(collateral: u128) -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, response, mut saved) = withdrawn_for_route(false, collateral);
    saved.pool.real_reserve0 = 0;
    saved.pool.real_reserve1 = 0;
    (input, response, saved)
}
fn token_calls(response: &Response) -> Vec<token::Message> {
    response
        .sync_messages
        .iter()
        .filter_map(|call| {
            (call.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
                .then(|| borsh::from_slice(&call.message).ok())
                .flatten()
        })
        .collect()
}
#[test]
fn reserve_withdrawal_records_unsold_inventory_and_recorded_collateral() {
    let (input, response, _) = withdrawn();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::Withdrawn {
            unsold: 800,
            collateral_due: 0
        }
    );
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(state.pool_id, state.curve_program_id))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&call.message).unwrap(),
        curve_core::Message::WithdrawReserves { now: 400 }
    );
    assert_eq!(state.creator_allocation, 100);
    assert_eq!(
        input.receiver.account_id,
        compute_factory_pda(id(40), id(7), [3; 32])
    );
}
#[test]
fn manual_settlement_pays_both_assets_without_burning_or_paying_surplus() {
    let (input, withdrawn, saved) = payout_ready(50);
    let (mut input, complete) = settlement(&input, &withdrawn, saved, 817, 67, false);
    let state = FactoryState::decode(complete.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.settlement_stage, SettlementStage::Complete);
    assert!(!state.creator_allocation_claimed);
    assert_eq!(
        token_calls(&complete),
        vec![
            token::Message::Transfer {
                definition: state.token_definition_id,
                recipient: id(42),
                amount: 800
            },
            token::Message::Transfer {
                definition: state.collateral_definition_id,
                recipient: id(42),
                amount: 50
            },
        ]
    );
    input.pre_state = complete.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 400,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::AlreadySettled));
}
#[test]
fn allocation_uses_saved_closure_instead_of_a_future_client_time() {
    for (lifecycle, expected) in [
        (pool::PoolLifecycle::Open, Some(400_000)),
        (pool::PoolLifecycle::Closed, None),
    ] {
        let (mut input, active) = active();
        let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        let mut saved = pool_state(&state, input.receiver, true);
        saved.pool.lifecycle = lifecycle;
        input.pre_state = active.post_state.unwrap();
        input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 999,
        })
        .unwrap();
        let response = handle(&input).unwrap();
        input = proof(&input, &response, id(42));
        let mut response = handle(&input).unwrap();
        for bytes in [
            borsh::to_vec(&saved).unwrap(),
            holding(state.creator_escrow_id, state.token_definition_id, 100),
        ] {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        assert_eq!(response.validity.timestamps.start(), expected);
        assert_eq!(response.validity.timestamps.end(), None);
    }
}

#[test]
fn creator_allocation_follows_the_role_to_its_current_holder_after_closure() {
    let (mut input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = active.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(99),
        now: 400,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages.last().unwrap().to,
        Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    input = proof(&input, &response, id(99));
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&pool_state(&state, input.receiver, true)).unwrap(),
        holding(state.creator_escrow_id, state.token_definition_id, 117),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert!(
        FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_claimed
    );
    let payout = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(state.creator_escrow_id, TOKEN_PROGRAM_ACCOUNT_ID))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<token::Message>(&payout.message).unwrap(),
        token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: id(99),
            amount: 100
        }
    );
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(99),
        now: 400,
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::AlreadyClaimed)));
}
fn adapter_state(factory: Actor, factory_state: &[u8]) -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory.account_id), id(9)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::Curve,
        })
        .unwrap(),
    };
    let response = handle_adapter(&input).unwrap();
    let call = response.sync_messages.last().unwrap();
    assert_eq!(call.to, factory);
    assert_eq!(
        borsh::from_slice::<Message>(&call.message).unwrap(),
        Message::ReadState
    );
    let input = reply(&input, &response, factory_state.to_vec());
    handle_adapter(&input).unwrap().post_state.unwrap().to_vec()
}
fn hook_ready() -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, withdrawn, mut saved) = withdrawn_for_route(true, 53);
    saved.pool.real_reserve0 = 0;
    saved.pool.real_reserve1 = 0;
    let (mut input, response) = settlement(&input, &withdrawn, saved.clone(), 817, 70, true);
    input = reply(
        &input,
        &response,
        adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
    );
    let response = handle(&input).unwrap();
    (input, response, saved)
}
#[test]
fn hook_preparation_saves_both_liabilities_and_forbids_manual_override() {
    let (mut input, response, _) = hook_ready();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady {
            unsold: 800,
            collateral_due: 53
        }
    );
    assert!(
        token_calls(&response)
            .iter()
            .all(|call| matches!(call, token::Message::Initialize { .. }))
    );
    input.pre_state = response.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 400,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
}
#[test]
fn hook_finalization_conserves_both_assets_and_binds_the_callback_hash() {
    let (input, ready, saved) = hook_ready();
    let (mut input, response) = settlement(&input, &ready, saved, 817, 70, true);
    input = reply(
        &input,
        &response,
        adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
    );
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady {
            unsold: 800,
            collateral_due: 53
        }
    );
    let custody = compute_graduation_pda(id(9), input.receiver.account_id);
    let calls = token_calls(&response);
    for (definition, adapter, creator, eligible) in [
        (state.token_definition_id, 200, 600, 800),
        (state.collateral_definition_id, 17, 36, 53),
    ] {
        assert!(calls.contains(&token::Message::Transfer {
            definition,
            recipient: custody,
            amount: adapter
        }));
        assert!(calls.contains(&token::Message::Transfer {
            definition,
            recipient: id(42),
            amount: creator
        }));
        assert_eq!(adapter + creator, eligible);
    }
    assert_eq!(calls.len(), 4);
    assert!(
        response
            .sync_messages
            .iter()
            .filter(|m| m.to.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID)
            .all(|m| m.pda_seeds.contains(&compute_factory_seed(id(40), [3; 32])))
    );
    let effects: Vec<_> = launch_events(&response)
        .into_iter()
        .flat_map(|event| match event.kind {
            SaleEventKindV1::Settlement { effects, .. } => effects,
            _ => vec![],
        })
        .collect();
    for (definition, expected) in [
        (state.token_definition_id, 800),
        (state.collateral_definition_id, 53),
    ] {
        let total: u128 = effects
            .iter()
            .filter_map(|effect| match effect {
                SettlementEffectV1::Payout {
                    definition: actual,
                    liability_discharged,
                    tokens_transferred,
                    ..
                } if actual == definition.value() => {
                    assert_eq!(liability_discharged, tokens_transferred);
                    Some(*tokens_transferred)
                }
                _ => None,
            })
            .sum();
        assert_eq!(total, expected);
    }
    let callback = response.sync_messages.last().unwrap();
    assert_eq!(callback.to, Actor::new(custody, id(9)));
    assert!(callback.pda_seeds.is_empty());
    let AdapterMessage::Graduate { request } = borsh::from_slice(&callback.message).unwrap() else {
        panic!("graduate")
    };
    assert_eq!(
        request.intent,
        GraduationIntent::Curve {
            launch_tokens: 200,
            collateral: 17
        }
    );
    assert_eq!(request.configuration, b"route");
    input.pre_state = response.post_state.unwrap();
    input.from = Some(Actor::new(custody, id(9)));
    let valid = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
        request_hash: request.hash(),
    })
    .unwrap();
    for intent in [
        GraduationIntent::Curve {
            launch_tokens: 201,
            collateral: 17,
        },
        GraduationIntent::Curve {
            launch_tokens: 200,
            collateral: 18,
        },
    ] {
        let mut changed = request.clone();
        changed.intent = intent;
        input.message = borsh::to_vec(&GraduationResult {
            factory: input.receiver.account_id,
            request_hash: changed.hash(),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    }
    input.message = valid.clone();
    input.from = Some(Actor::new(custody, id(99)));
    assert_eq!(handle(&input).err(), Some(Error::Unauthorized));
    input.from = Some(Actor::new(custody, id(9)));
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidMessage));
    input.message = valid;
    let complete = handle(&input).unwrap();
    assert_eq!(
        FactoryState::decode(complete.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Graduated
    );
    assert!(matches!(
        &launch_events(&complete)[..],
        [
            SaleEventV1 {
                kind: SaleEventKindV1::Settlement {
                    step: SettlementStepV1::Completed,
                    route: Some(SettlementRouteV1::Graduation),
                    ..
                },
                ..
            },
            SaleEventV1 {
                kind: SaleEventKindV1::Graduated {
                    launch_tokens: 200,
                    collateral: 17,
                    ..
                },
                ..
            }
        ]
    ));
}
#[test]
fn traded_pool_settlement_checks_immutable_k_instead_of_initial_virtual_reserves() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, true);
    saved
        .pool
        .swap_exact_input(pool::ExactInputSwap {
            token_in: pool::TokenSide::Token1,
            amount_in: 10,
            min_amount_out: 1,
            pool_fee_bps: 0,
            protocol_fee_bps: 0,
            now: 100,
        })
        .unwrap();
    assert_ne!(saved.pool.virtual_reserve0, 1000);
    let (_, ready) = settlement(&input, &active, saved.clone(), 100, 0, false);
    assert_eq!(
        FactoryState::decode(ready.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Ready
    );
    let mut input = input.clone();
    input.pre_state = active.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
        creator: id(42),
        now: 400,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&saved).unwrap(),
        holding(state.creator_escrow_id, state.token_definition_id, 100),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert!(
        FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .creator_allocation_claimed
    );
}
#[test]
fn public_supply_and_address_rules_preserve_the_original_factory_contract() {
    assert_eq!(total_supply(800, 0, 100), Ok(900));
    assert_eq!(total_supply(0, 0, 100), Err(CreateError::SaleReserveZero));
    assert_eq!(
        total_supply(u128::MAX, 0, 1),
        Err(CreateError::SupplyOverflow)
    );
    assert_eq!(
        validate_curve_parameters(800, 800, 100),
        Err(CreateError::VirtualTokenReserveNotAboveSaleReserve)
    );
    assert_eq!(
        validate_curve_parameters(800, 1000, 0),
        Err(CreateError::VirtualReserveZero)
    );
    assert_eq!(
        validate_curve_parameters(800, pool::VIRTUAL_RESERVE_BOUND, 100),
        Err(CreateError::VirtualReserveAboveBound)
    );
    assert!(TradeMode::try_from(2).is_err());
    let mut invalid_mode = creation();
    let mut message: Message = borsh::from_slice(&invalid_mode.message).unwrap();
    let Message::CreateFactoryPool { trade_mode, .. } = &mut message else {
        unreachable!()
    };
    *trade_mode = 2;
    invalid_mode.message = borsh::to_vec(&message).unwrap();
    assert_eq!(handle(&invalid_mode).err(), Some(Error::InvalidTerms));
    let addresses = [
        compute_factory_pda(id(40), id(7), [3; 32]),
        compute_definition_pda(id(40), id(7), [3; 32]),
        compute_mint_pda(id(40), id(7), [3; 32]),
        compute_metadata_pda(id(40), id(7), [3; 32]),
        compute_escrow_pda(id(40), id(7), [3; 32]),
    ];
    for (index, address) in addresses.iter().enumerate() {
        assert!(!addresses[..index].contains(address));
    }
    assert_ne!(
        compute_factory_pda(id(40), id(7), [3; 32]),
        compute_factory_pda(id(41), id(7), [3; 32])
    );
    assert_ne!(
        compute_factory_pda(id(40), id(7), [3; 32]),
        compute_factory_pda(id(40), id(7), [4; 32])
    );
    assert_ne!(
        compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
        compute_curve_creator_role_pda(id(40), id(7), [4; 32])
    );
}

#[test]
fn expiry_is_proved_separately_from_the_supplied_zero_lower_bound() {
    let (mut input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let saved = pool_state(&state, input.receiver, true);
    input.pre_state = active.post_state.unwrap();
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 0,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&saved).unwrap(),
        holding(input.receiver.account_id, state.token_definition_id, 100),
        holding(input.receiver.account_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    assert_eq!(response.validity.timestamps.start(), Some(400_000));
    assert_eq!(response.validity.timestamps.end(), None);
}

#[test]
fn curve_creation_reads_namespace_before_bootstrapping_its_creator_role() {
    let input = creation();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6))
    );
}

#[test]
fn curve_creation_bootstraps_a_transfer_only_creator_role_with_the_mint() {
    let (_, response) = minted();
    assert_eq!(response.validity.timestamps.start(), None);
    assert_eq!(response.validity.timestamps.end(), Some(400_000));
    let call = response
        .sync_messages
        .iter()
        .find(|call| call.to.program_account_id == AUTHORITY_PROGRAM_ACCOUNT_ID)
        .expect("creator role bootstrap");
    assert_eq!(
        borsh::from_slice::<authority::Message>(&call.message).unwrap(),
        authority::Message::BootstrapTransferOnly { authority: id(42) }
    );
    assert_eq!(call.pda_seeds.len(), 1);
    let (_, response) = minted();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.total_supply, 900);
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == state.token_definition_id)
        .unwrap();
    let token::Message::Create {
        supply, recipient, ..
    } = borsh::from_slice(&call.message).unwrap()
    else {
        panic!("mint")
    };
    assert_eq!(supply, 900);
    assert_eq!(recipient, compute_mint_pda(id(40), id(7), [3; 32]));
}

#[test]
fn curve_rejects_a_creator_role_from_another_namespace() {
    let (mut input, response) = minted();
    let mut state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creator_role_id = compute_curve_creator_role_pda(id(41), id(7), [3; 32]);
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::ContinueCreation {
        creator: id(42),
        now: 100,
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::WrongAccount)));
}

#[test]
fn continuation_checks_the_creator_before_any_custody_call() {
    let (mut input, response) = minted();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ContinueCreation {
        creator: id(42),
        now: 100,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(id(42), AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    assert_eq!(
        borsh::from_slice::<authority::Message>(&response.sync_messages[0].message).unwrap(),
        authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver
        }
    );
}

fn role_route(
    role: Actor,
    state: &mut Vec<u8>,
    mut sender: Actor,
    mut call: lee_core::program::SyncMessage,
    signers: &[AccountId],
    mut grant: bool,
    callback: Option<Actor>,
) -> Result<Option<(Actor, Vec<u8>)>, lez_compat_core::Error> {
    loop {
        if Some(call.to) == callback {
            return Ok(Some((sender, call.message.to_vec())));
        }
        let receiver = call.to;
        let response = authority::handle(&ReceiveInput {
            receiver,
            from: Some(sender),
            is_authorized: grant || signers.contains(&receiver.account_id),
            pre_state: if receiver == role {
                state.clone().into()
            } else {
                vec![].into()
            },
            message: call.message.to_vec(),
        })?;
        if receiver == role
            && let Some(post_state) = response.post_state
        {
            *state = post_state.to_vec();
        }
        let Some(next) = response.sync_messages.into_iter().next() else {
            return Ok(None);
        };
        sender = receiver;
        call = next;
        grant = false;
    }
}

#[test]
fn creator_transfer_requires_consent_and_moves_all_remaining_factory_rights() {
    let (input, minted) = minted();
    let original = FactoryState::decode(minted.post_state.as_ref().unwrap()).unwrap();
    let role = Actor::new(original.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID);
    let bootstrap = minted
        .sync_messages
        .iter()
        .find(|call| call.to == role)
        .unwrap()
        .clone();
    let mut role_state = vec![];
    assert!(matches!(
        role_route(
            role,
            &mut vec![],
            input.receiver,
            bootstrap.clone(),
            &[],
            true,
            None
        ),
        Err(lez_compat_core::Error::Unauthorized)
    ));
    role_route(
        role,
        &mut role_state,
        input.receiver,
        bootstrap,
        &[id(42)],
        true,
        None,
    )
    .unwrap();
    let transfer = lee_core::program::SyncMessage::new(
        Actor::new(id(42), AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Transfer {
            role: role.account_id,
            successor: id(99),
        },
    );
    assert!(matches!(
        role_route(
            role,
            &mut role_state.clone(),
            input.receiver,
            transfer.clone(),
            &[id(42)],
            false,
            None
        ),
        Err(lez_compat_core::Error::Unauthorized)
    ));
    role_route(
        role,
        &mut role_state,
        input.receiver,
        transfer,
        &[id(42), id(99)],
        false,
        None,
    )
    .unwrap();

    for (creation_stage, settlement_stage, command) in [
        (
            CreationStage::Minted,
            SettlementStage::Unstarted,
            Message::ContinueCreation {
                creator: id(99),
                now: 100,
            },
        ),
        (
            CreationStage::Active,
            SettlementStage::Unstarted,
            Message::SetPaused {
                creator: id(99),
                paused: true,
            },
        ),
        (
            CreationStage::Active,
            SettlementStage::Unstarted,
            Message::ClaimCreatorAllocation {
                creator: id(99),
                now: 400,
            },
        ),
        (
            CreationStage::Active,
            SettlementStage::Withdrawn {
                unsold: 100,
                collateral_due: 50,
            },
            Message::WithdrawFactoryProceeds {
                creator: id(99),
                now: 400,
            },
        ),
    ] {
        let mut state = original.clone();
        state.creation_stage = creation_stage;
        state.settlement_stage = settlement_stage;
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message = borsh::to_vec(&command).unwrap();
        let response = handle(&request).unwrap();
        let check = response.sync_messages.last().unwrap().clone();
        let (sender, message) = role_route(
            role,
            &mut role_state,
            request.receiver,
            check.clone(),
            &[id(99)],
            false,
            Some(request.receiver),
        )
        .unwrap()
        .unwrap();
        let mut callback = request.clone();
        callback.from = Some(sender);
        callback.message = message;
        callback.pre_state = response.post_state.unwrap();
        let resumed = handle(&callback).unwrap();
        let current = FactoryState::decode(resumed.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(
            (
                current.namespace,
                current.launch_salt,
                current.creator_role_id
            ),
            (id(40), [3; 32], role.account_id)
        );
        assert_eq!(
            (
                current.sale_reserve,
                current.creator_allocation,
                current.total_supply
            ),
            (800, 100, 900)
        );
        assert_eq!(
            (
                current.virtual_token_reserve,
                current.virtual_collateral_reserve,
                current.end_timestamp
            ),
            (1000, 100, Some(400))
        );
        assert_eq!(
            (
                current.token_definition_id,
                current.pool_id,
                current.creator_escrow_id
            ),
            (
                state.token_definition_id,
                state.pool_id,
                state.creator_escrow_id
            )
        );
        if matches!(command, Message::WithdrawFactoryProceeds { .. }) {
            let mut saved = pool_state(&state, input.receiver, true);
            saved.pool.real_reserve0 = 0;
            saved.pool.real_reserve1 = 0;
            let mut response = resumed;
            for bytes in [
                borsh::to_vec(&saved).unwrap(),
                holding(input.receiver.account_id, state.token_definition_id, 100),
                holding(
                    input.receiver.account_id,
                    state.collateral_definition_id,
                    50,
                ),
            ] {
                callback = reply(&callback, &response, bytes);
                response = handle(&callback).unwrap();
            }
            assert_eq!(
                borsh::from_slice::<token::Message>(&response.sync_messages[0].message).unwrap(),
                token::Message::Transfer {
                    definition: state.token_definition_id,
                    recipient: id(99),
                    amount: 100
                }
            );
        }

        let previous_command = match command {
            Message::SetPaused { paused, .. } => Message::SetPaused {
                creator: id(42),
                paused,
            },
            Message::ContinueCreation { now, .. } => Message::ContinueCreation {
                creator: id(42),
                now,
            },
            Message::ClaimCreatorAllocation { now, .. } => Message::ClaimCreatorAllocation {
                creator: id(42),
                now,
            },
            Message::WithdrawFactoryProceeds { now, .. } => Message::WithdrawFactoryProceeds {
                creator: id(42),
                now,
            },
            _ => unreachable!(),
        };
        request.message = borsh::to_vec(&previous_command).unwrap();
        let rejected = handle(&request).unwrap();
        assert!(matches!(
            role_route(
                role,
                &mut role_state.clone(),
                request.receiver,
                rejected.sync_messages.last().unwrap().clone(),
                &[id(42)],
                false,
                Some(request.receiver)
            ),
            Err(lez_compat_core::Error::Unauthorized)
        ));
        request.message = borsh::to_vec(&match command {
            Message::SetPaused { paused, .. } => Message::SetPaused {
                creator: id(40),
                paused,
            },
            Message::ContinueCreation { now, .. } => Message::ContinueCreation {
                creator: id(40),
                now,
            },
            Message::ClaimCreatorAllocation { now, .. } => Message::ClaimCreatorAllocation {
                creator: id(40),
                now,
            },
            Message::WithdrawFactoryProceeds { now, .. } => Message::WithdrawFactoryProceeds {
                creator: id(40),
                now,
            },
            _ => unreachable!(),
        })
        .unwrap();
        let admin = handle(&request).unwrap();
        assert!(matches!(
            role_route(
                role,
                &mut role_state.clone(),
                request.receiver,
                admin.sync_messages.last().unwrap().clone(),
                &[id(40)],
                false,
                Some(request.receiver)
            ),
            Err(lez_compat_core::Error::Unauthorized)
        ));
    }
}

#[test]
fn creator_callbacks_reject_wrong_program_role_holder_and_legacy_nft_proofs() {
    let (input, minted) = minted();
    for command in [
        Message::SetPaused {
            creator: id(42),
            paused: true,
        },
        Message::ContinueCreation {
            creator: id(42),
            now: 100,
        },
        Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 400,
        },
        Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 400,
        },
    ] {
        let mut request = input.clone();
        let mut state = FactoryState::decode(minted.post_state.as_ref().unwrap()).unwrap();
        if !matches!(command, Message::ContinueCreation { .. }) {
            state.creation_stage = CreationStage::Active;
        }
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message = borsh::to_vec(&command).unwrap();
        let response = handle(&request).unwrap();
        let valid = proof(&request, &response, id(42));
        for (sender, role, holder) in [
            (
                Actor::new(state.creator_role_id, id(99)),
                state.creator_role_id,
                id(42),
            ),
            (
                Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID),
                state.creator_role_id,
                id(42),
            ),
            (
                Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                id(99),
                id(42),
            ),
            (
                Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                state.creator_role_id,
                id(99),
            ),
        ] {
            let mut forged = valid.clone();
            forged.from = Some(sender);
            forged.message = borsh::to_vec(&authority::Authorized {
                role,
                authority: holder,
            })
            .unwrap();
            assert!(matches!(handle(&forged), Err(Error::Unauthorized)));
        }
        let mut legacy = valid.clone();
        legacy.from = Some(Actor::new(
            token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, id(42), id(43)),
            TOKEN_PROGRAM_ACCOUNT_ID,
        ));
        legacy.message = borsh::to_vec(&token::NftHolder {
            definition: id(43),
            owner: id(42),
        })
        .unwrap();
        assert!(matches!(handle(&legacy), Err(Error::Unauthorized)));
        let _ = handle(&valid).unwrap();
    }
}

#[test]
fn creator_role_addresses_are_separate_from_other_sales_roles_and_namespace_admin() {
    let role = compute_curve_creator_role_pda(id(40), id(7), [3; 32]);
    for other in [
        compute_curve_creator_role_pda(id(41), id(7), [3; 32]),
        compute_curve_creator_role_pda(id(40), id(7), [4; 32]),
        compute_curve_creator_role_pda(id(40), id(8), [3; 32]),
        compute_creator_role_pda(id(40), id(7), [3; 32]),
        curve_core::compute_config_pda(id(40), id(6)),
    ] {
        assert_ne!(role, other);
    }
}

#[test]
fn curve_creation_wire_rejects_the_removed_creator_nft_argument() {
    let mut input = creation();
    let legacy_argument = [43; 32];
    input.message.splice(
        input.message.len() - 8..input.message.len() - 8,
        legacy_argument,
    );
    assert!(matches!(handle(&input), Err(Error::InvalidMessage)));
}

#[test]
fn curve_pause_policy_survives_setup_and_checks_policy_instead_of_pause_value() {
    for policy in [PausePolicy::Disabled, PausePolicy::AuthorityControlled] {
        let mut input = creation();
        let mut message: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool { pause_policy, .. } = &mut message {
            *pause_policy = policy;
        }
        input.message = borsh::to_vec(&message).unwrap();
        let mut mint = handle(&input).unwrap();
        for bytes in [config(), definition()] {
            assert_eq!(
                FactoryState::decode(mint.post_state.as_ref().unwrap())
                    .unwrap()
                    .pause_policy,
                policy
            );
            input = reply(&input, &mint, bytes);
            mint = handle(&input).unwrap();
        }
        let (input, allocated) = continued(&input, &mint, 900, 0, 0);
        assert_eq!(
            FactoryState::decode(allocated.post_state.as_ref().unwrap())
                .unwrap()
                .pause_policy,
            policy
        );
        let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
        assert_eq!(
            FactoryState::decode(prepared.post_state.as_ref().unwrap())
                .unwrap()
                .pause_policy,
            policy
        );
        let create = prepared
            .sync_messages
            .iter()
            .find_map(|call| borsh::from_slice::<curve_core::Message>(&call.message).ok())
            .unwrap();
        assert!(
            matches!(create, curve_core::Message::CreatePool { pause_policy, .. } if pause_policy == policy)
        );
        for control in [
            curve_core::TradeControl::Unrestricted,
            curve_core::TradeControl::Running,
            curve_core::TradeControl::Paused,
        ] {
            let (mut input, mut response) = continued(&input, &prepared, 0, 800, 100);
            let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
            let mut saved = pool_state(&state, input.receiver, false);
            saved.trade_control = control;
            input = reply(&input, &response, borsh::to_vec(&saved).unwrap());
            response = handle(&input).unwrap();
            input = reply(
                &input,
                &response,
                holding(state.pool_id, state.token_definition_id, 0),
            );
            let result = handle(&input);
            if control.policy() != policy {
                assert_eq!(result.err(), Some(Error::InvalidState));
            } else {
                let active =
                    FactoryState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(active.creation_stage, CreationStage::Active);
                assert_eq!(active.pause_policy, policy);
            }
        }
    }
}

#[test]
fn curve_pause_checks_current_creator_on_every_call_and_forwards_only_to_bound_pool() {
    let (mut input, active) = active();
    let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    state.pause_policy = PausePolicy::AuthorityControlled;
    for paused in [true, false] {
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::SetPaused {
            creator: id(99),
            paused,
        })
        .unwrap();
        let response = handle(&input).unwrap();
        assert_eq!(response.sync_messages.len(), 1);
        let check = &response.sync_messages[0];
        assert_eq!(check.to, Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID));
        assert_eq!(
            borsh::from_slice::<authority::Message>(&check.message).unwrap(),
            authority::Message::Check {
                role: state.creator_role_id,
                callback: input.receiver
            }
        );
        assert!(check.pda_seeds.is_empty());
        assert_eq!(
            handle(&proof(&input, &response, id(42))).err(),
            Some(Error::Unauthorized)
        );
        let response = handle(&proof(&input, &response, id(99))).unwrap();
        assert!(response.events.is_empty());
        assert_eq!(response.sync_messages.len(), 1);
        let call = &response.sync_messages[0];
        assert_eq!(call.to, Actor::new(state.pool_id, state.curve_program_id));
        assert_eq!(
            borsh::from_slice::<curve_core::Message>(&call.message).unwrap(),
            curve_core::Message::SetPaused { paused }
        );
        assert_eq!(
            call.pda_seeds,
            [compute_factory_seed(state.namespace, state.launch_salt)].into()
        );
        assert_eq!(
            FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap(),
            state
        );
    }
    state.creation_stage = CreationStage::PoolPrepared;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    assert_eq!(handle(&input).err(), Some(Error::CreationPending));
    input.message = borsh::to_vec(&Message::SetWeightedPaused {
        creator: id(42),
        paused: true,
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidState));
}

#[test]
fn curve_creation_checks_the_ending_matrix_before_reads() {
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for end in [None, Some(400)] {
            let mut input = creation();
            let mut command: Message = borsh::from_slice(&input.message).unwrap();
            if let Message::CreateFactoryPool {
                ending_policy,
                end_timestamp,
                ..
            } = &mut command
            {
                *ending_policy = ending;
                *end_timestamp = end;
            }
            input.message = borsh::to_vec(&command).unwrap();
            let result = handle(&input);
            let valid = match ending {
                EndingPolicy::Manual => false,
                EndingPolicy::Scheduled | EndingPolicy::Either => end.is_some(),
                EndingPolicy::Depletion => end.is_none(),
            };
            if valid {
                let state =
                    FactoryState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!((state.ending_policy, state.end_timestamp), (ending, end));
            } else {
                assert_eq!(result.err(), Some(Error::InvalidTerms));
            }
        }
    }
}

#[test]
fn curve_timestamp_overflow_rejects_before_setup_reads_or_minting() {
    let mut input = creation();
    let mut command: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool { end_timestamp, .. } = &mut command {
        *end_timestamp = Some(u64::MAX);
    }
    input.message = borsh::to_vec(&command).unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
}

#[test]
fn curve_rejects_malformed_saved_policies_before_resuming_setup() {
    let mut input = creation();
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for case in 0..3 {
        let mut changed = state.clone();
        match case {
            0 => changed.ending_policy = EndingPolicy::Manual,
            1 => changed.ending_policy = EndingPolicy::Depletion,
            _ => changed.end_timestamp = Some(u64::MAX),
        }
        let bytes = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            FactoryState::decode(&bytes).err(),
            Some(Error::InvalidState)
        );
        input = reply(&input, &response, config());
        input.pre_state = bytes.into();
        assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    }
}

#[test]
fn curve_activation_rejects_a_valid_direct_pool_ending_policy() {
    let (input, mint) = minted();
    let (input, allocated) = continued(&input, &mint, 900, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
    let (mut input, mut response) = continued(&input, &prepared, 0, 800, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, false);
    saved.pool.ending_policy = EndingPolicy::Manual;
    saved.pool.close_timestamp = None;
    saved.pool.close_on_depletion = None;
    input = reply(&input, &response, borsh::to_vec(&saved).unwrap());
    response = handle(&input).unwrap();
    input = reply(
        &input,
        &response,
        holding(state.pool_id, state.token_definition_id, 0),
    );
    assert_eq!(handle(&input).err(), Some(Error::InvalidCustody));
}

#[test]
fn curve_allocation_rejects_changed_inner_pool_policy() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    for case in 0..2 {
        let mut saved = pool_state(&state, input.receiver, true);
        if case == 0 {
            saved.pool.ending_policy = EndingPolicy::Manual;
            saved.pool.close_timestamp = None;
            saved.pool.close_on_depletion = None;
        } else {
            saved.pool.ending_policy = EndingPolicy::Scheduled;
            saved.pool.close_on_depletion = None;
        }
        let mut request = input.clone();
        request.pre_state = active.post_state.clone().unwrap();
        request.from = None;
        request.message = borsh::to_vec(&Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 999,
        })
        .unwrap();
        let response = handle(&request).unwrap();
        request = proof(&request, &response, id(42));
        let mut response = handle(&request).unwrap();
        request = reply(&request, &response, borsh::to_vec(&saved).unwrap());
        response = handle(&request).unwrap();
        request = reply(
            &request,
            &response,
            holding(state.creator_escrow_id, state.token_definition_id, 100),
        );
        assert_eq!(handle(&request).err(), Some(Error::InvalidCustody));
    }
}

#[test]
fn selected_ending_controls_allocation_proceeds_and_hook_payouts() {
    for ending in [
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        for remaining in [0, 800] {
            for action in 0..3 {
                let (mut input, active) = active_for_route(action == 2);
                let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
                state.ending_policy = ending;
                state.end_timestamp = if ending == EndingPolicy::Depletion {
                    None
                } else {
                    Some(400)
                };
                let mut saved = pool_state(&state, input.receiver, true);
                saved.pool.real_reserve0 = remaining;
                input.pre_state = borsh::to_vec(&state).unwrap().into();
                input.from = None;
                let command = match action {
                    0 => Message::ClaimCreatorAllocation {
                        creator: id(42),
                        now: 999,
                    },
                    1 => Message::WithdrawFactoryProceeds {
                        creator: id(42),
                        now: 999,
                    },
                    _ => Message::ExecuteGraduationHook {
                        creator_hint: Some(id(42)),
                    },
                };
                input.message = borsh::to_vec(&command).unwrap();
                let mut response = handle(&input).unwrap();
                input = if action == 2 {
                    resolved(&input, &response, id(42))
                } else {
                    proof(&input, &response, id(42))
                };
                response = handle(&input).unwrap();
                let mut values = vec![borsh::to_vec(&saved).unwrap()];
                if action == 0 {
                    values.push(holding(
                        state.creator_escrow_id,
                        state.token_definition_id,
                        100,
                    ));
                } else {
                    values.extend([
                        holding(input.receiver.account_id, state.token_definition_id, 100),
                        holding(input.receiver.account_id, state.collateral_definition_id, 0),
                    ]);
                }
                let mut result = Ok(response);
                for value in values {
                    input = reply(&input, result.as_ref().unwrap(), value);
                    result = handle(&input);
                }
                if ending == EndingPolicy::Depletion && remaining != 0 {
                    assert_eq!(result.err(), Some(Error::NotClosed));
                    continue;
                }
                let response = result.unwrap();
                let early = remaining == 0 && ending != EndingPolicy::Scheduled;
                assert_eq!(
                    response.validity.timestamps.start(),
                    if early { None } else { Some(400_000) }
                );
                assert_eq!(response.validity.timestamps.end(), None);
                assert_eq!(response.validity.timestamps.is_valid_for(399_999), early);
                assert!(response.validity.timestamps.is_valid_for(400_000));
                let after = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
                assert_eq!(after.creator_allocation, 100);
                assert_eq!(after.sale_reserve, 800);
            }
        }
    }
}

#[test]
fn curve_setup_keeps_each_selected_ending_and_its_depletion_side() {
    for ending in [
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        let end = if ending == EndingPolicy::Depletion {
            None
        } else {
            Some(400)
        };
        if let Message::CreateFactoryPool {
            ending_policy,
            end_timestamp,
            ..
        } = &mut command
        {
            *ending_policy = ending;
            *end_timestamp = end;
        }
        input.message = borsh::to_vec(&command).unwrap();
        let mut response = handle(&input).unwrap();
        for bytes in [config(), definition()] {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        assert_eq!(
            response.validity.timestamps.end(),
            end.map(|end| end * 1000)
        );
        let (input, allocated) = continued(&input, &response, 900, 0, 0);
        let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
        let message: curve_core::Message =
            borsh::from_slice(&prepared.sync_messages[0].message).unwrap();
        let expected_side = if ending == EndingPolicy::Scheduled {
            None
        } else {
            Some(0)
        };
        assert!(matches!(message, curve_core::Message::CreatePool {
            ending_policy, close_timestamp, close_on_depletion, token0_amount: 800, token1_amount: 0, ..
        } if ending_policy == ending && close_timestamp == end && close_on_depletion == expected_side));
        let (mut input, mut response) = continued(&input, &prepared, 0, 800, 100);
        let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
        for bytes in [
            borsh::to_vec(&pool_state(&state, input.receiver, false)).unwrap(),
            holding(state.pool_id, state.token_definition_id, 0),
        ] {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        let active = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(active.creation_stage, CreationStage::Active);
        assert_eq!(active.ending_policy, ending);
        assert_eq!(active.end_timestamp, end);
        assert_eq!(active.creator_allocation, 100);
    }
}

#[test]
fn curve_settlement_progress_and_wire_tags_match_the_payout_stages() {
    for (stage, progress, tag, payload) in [
        (
            SettlementStage::Unstarted,
            SettlementProgress::Unselected,
            0,
            vec![],
        ),
        (
            SettlementStage::Ready,
            SettlementProgress::Unselected,
            1,
            vec![],
        ),
        (
            SettlementStage::Withdrawn {
                unsold: 17,
                collateral_due: 29,
            },
            SettlementProgress::Unselected,
            2,
            vec![17_u128, 29],
        ),
        (
            SettlementStage::HookReady {
                unsold: 17,
                collateral_due: 29,
            },
            SettlementProgress::Unselected,
            3,
            vec![17, 29],
        ),
        (
            SettlementStage::Complete,
            SettlementProgress::ManualComplete,
            4,
            vec![],
        ),
        (
            SettlementStage::Graduated,
            SettlementProgress::Graduated,
            5,
            vec![],
        ),
    ] {
        assert_eq!(stage.progress(), progress);
        assert_eq!(
            stage.is_manual_settlement_complete(),
            progress.is_complete()
        );
        let mut expected = vec![tag];
        for amount in payload {
            expected.extend_from_slice(&amount.to_le_bytes());
        }
        assert_eq!(borsh::to_vec(&stage).unwrap(), expected);
        assert_eq!(
            borsh::from_slice::<SettlementStage>(&expected).unwrap(),
            stage
        );
    }
}
#[test]
fn configured_adapter_blocks_manual_settlement_at_every_stage_even_with_zero_shares() {
    for stage in [
        SettlementStage::Unstarted,
        SettlementStage::Ready,
        SettlementStage::Withdrawn {
            unsold: 0,
            collateral_due: 0,
        },
        SettlementStage::HookReady {
            unsold: 0,
            collateral_due: 0,
        },
        SettlementStage::Complete,
        SettlementStage::Graduated,
    ] {
        let (mut input, active) = active_for_route(true);
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.settlement_stage = stage;
        state.graduation_policy = GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 0,
        };
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 400,
        })
        .unwrap();
        assert_eq!(
            handle(&input).err(),
            Some(Error::InvalidTerms),
            "stage {stage:?}"
        );
    }
}
#[test]
fn hook_requires_a_saved_adapter_and_manual_completion_cannot_reopen_it() {
    for stage in [
        SettlementStage::Unstarted,
        SettlementStage::Ready,
        SettlementStage::Withdrawn {
            unsold: 0,
            collateral_due: 0,
        },
        SettlementStage::Complete,
        SettlementStage::Graduated,
    ] {
        let (mut input, active) = active();
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.settlement_stage = stage;
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message = borsh::to_vec(&Message::ExecuteGraduationHook {
            creator_hint: Some(id(42)),
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::HookNotConfigured));
    }
}

fn launch_events(response: &Response) -> Vec<SaleEventV1> {
    response
        .events
        .iter()
        .map(|event| {
            let payload: SaleEventV1 = borsh::from_slice(&event.data).unwrap();
            assert_eq!(
                event.selector,
                curve_core::sale_event(payload.clone()).selector
            );
            payload
        })
        .collect()
}

#[test]
fn curve_created_event_names_the_launch_and_immutable_terms_once() {
    let input = creation();
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        launch_events(&response),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::Curve,
            EventSubjectV1::Launch {
                factory: *input.receiver.account_id.value(),
                pool: *state.pool_id.value()
            },
            SaleEventKindV1::Created {
                creator: *id(42).value(),
                creator_role: *state.creator_role_id.value(),
                token_definition: *state.token_definition_id.value(),
                collateral_definition: *state.collateral_definition_id.value(),
                graduation_reserve: state.graduation_reserve,
                sale_inventory: 800,
                creator_allocation: 100,
                graduation_policy: GraduationPolicy {
                    launch_bps: 0,
                    collateral_bps: 0
                },
                collateral_seed: 0,
                start_timestamp: None,
                end_timestamp: Some(400),
                policies: EventPoliciesV1 {
                    ending: EndingPolicy::Either,
                    pause: PausePolicy::Disabled,
                    sell_fee_bps: 0
                }
            }
        )]
    );
    let (_, minted) = minted();
    assert!(minted.events.is_empty());
    let mut replay = input;
    replay.pre_state = minted.post_state.unwrap();
    assert_eq!(handle(&replay).err(), Some(Error::AlreadyInitialized));
}

#[test]
fn curve_allocation_read_emits_only_actual_allocation_payment() {
    for (amount, self_recipient) in [(0, false), (100, false), (100, true)] {
        let (mut input, active) = active();
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.creator_allocation = amount;
        state.total_supply =
            total_supply(state.sale_reserve, state.graduation_reserve, amount).unwrap();
        let pool = pool_state(&state, input.receiver, true);
        let creator = if self_recipient {
            state.creator_escrow_id
        } else {
            id(42)
        };
        input.pre_state = borsh::to_vec(&state).unwrap().into();
        input.from = None;
        input.message =
            borsh::to_vec(&Message::ClaimCreatorAllocation { creator, now: 0 }).unwrap();
        let response = handle(&input).unwrap();
        input = proof(&input, &response, creator);
        let mut response = handle(&input).unwrap();
        let mut values = vec![borsh::to_vec(&pool).unwrap()];
        if amount != 0 {
            values.push(holding(
                state.creator_escrow_id,
                state.token_definition_id,
                amount,
            ));
        }
        for bytes in values {
            input = reply(&input, &response, bytes);
            response = handle(&input).unwrap();
        }
        let events = launch_events(&response);
        if amount == 0 || self_recipient {
            assert!(events.is_empty());
        } else {
            assert!(matches!(
                &events[..],
                [SaleEventV1 {
                    kind: SaleEventKindV1::AllocationReleased {
                        amount: 100,
                        release: AllocationReleaseV1::AfterEnd,
                        ..
                    },
                    ..
                }]
            ));
        }
        assert_eq!(response.validity.timestamps.start(), Some(400_000));
    }
}

#[test]
fn curve_manual_self_recipient_reports_discharged_claim_without_token_movement() {
    let (mut input, withdrawn, pool) = payout_ready(0);
    let state = FactoryState::decode(withdrawn.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = withdrawn.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: input.receiver.account_id,
        now: 0,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = proof(&input, &response, input.receiver.account_id);
    let mut response = handle(&input).unwrap();
    for bytes in [
        borsh::to_vec(&pool).unwrap(),
        holding(input.receiver.account_id, state.token_definition_id, 800),
        holding(input.receiver.account_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    let events = launch_events(&response);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { effects, .. }, .. }, SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Completed, .. }, .. }] if matches!(&effects[..], [SettlementEffectV1::Payout { liability_discharged: 800, tokens_transferred: 0, .. }]))
    );
}

#[test]
fn retained_curve_invariant_binds_original_product_and_allows_checked_growth() {
    let (input, mut active) = active();
    let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    state.sell_fee_bps = 125;
    state.trade_mode = TradeMode::BuyAndSell;
    active.post_state = Some(borsh::to_vec(&state).unwrap().into());
    let mut saved = pool_state(&state, input.receiver, true);
    for (side, amount) in [
        (pool::TokenSide::Token1, 100),
        (pool::TokenSide::Token0, 100),
    ] {
        saved
            .pool
            .swap_exact_input(pool::ExactInputSwap {
                token_in: side,
                amount_in: amount,
                min_amount_out: 1,
                pool_fee_bps: 0,
                protocol_fee_bps: 0,
                now: 100,
            })
            .unwrap();
    }
    assert!(saved.pool.k > saved.pool.initial_k);
    for hook in [false, true] {
        let mut routed = active.clone();
        let mut terms = state.clone();
        if hook {
            terms.graduation_hook = Some(GraduationHook {
                program_id: id(9),
                configuration: b"route".to_vec(),
            });
        }
        routed.post_state = Some(borsh::to_vec(&terms).unwrap().into());
        let (_, ready) = settlement(&input, &routed, saved.clone(), 100, 0, hook);
        assert_eq!(
            FactoryState::decode(ready.post_state.as_ref().unwrap())
                .unwrap()
                .settlement_stage,
            SettlementStage::Ready
        );
    }
    for case in 0..5 {
        let mut forged = saved.clone();
        match case {
            0 => forged.pool.sell_fee_bps += 1,
            1 => forged.pool.initial_k -= 1,
            2 => forged.pool.k = forged.pool.initial_k - 1,
            3 => forged.pool.virtual_reserve1 = 1,
            _ => {
                forged.pool.sell_fee_bps = 0;
            }
        }
        for command in [
            Message::ClaimCreatorAllocation {
                creator: id(42),
                now: 400,
            },
            Message::WithdrawFactoryProceeds {
                creator: id(42),
                now: 400,
            },
            Message::ExecuteGraduationHook {
                creator_hint: Some(id(42)),
            },
        ] {
            let allocation = matches!(command, Message::ClaimCreatorAllocation { .. });
            let hook = matches!(command, Message::ExecuteGraduationHook { .. });
            let mut request = input.clone();
            request.from = None;
            let mut terms = state.clone();
            if hook {
                terms.graduation_hook = Some(GraduationHook {
                    program_id: id(9),
                    configuration: b"route".to_vec(),
                });
            }
            request.pre_state = borsh::to_vec(&terms).unwrap().into();
            request.message = borsh::to_vec(&command).unwrap();
            let response = handle(&request).unwrap();
            request = if hook {
                resolved(&request, &response, id(42))
            } else {
                proof(&request, &response, id(42))
            };
            let mut response = handle(&request).unwrap();
            request = reply(&request, &response, borsh::to_vec(&forged).unwrap());
            if allocation {
                response = handle(&request).unwrap();
                request = reply(
                    &request,
                    &response,
                    holding(state.creator_escrow_id, state.token_definition_id, 100),
                );
            }
            if !allocation {
                response = handle(&request).unwrap();
                request = reply(
                    &request,
                    &response,
                    holding(input.receiver.account_id, state.token_definition_id, 100),
                );
                response = handle(&request).unwrap();
                request = reply(
                    &request,
                    &response,
                    holding(input.receiver.account_id, state.collateral_definition_id, 0),
                );
            }
            assert!(handle(&request).is_err(), "case {case}");
        }
    }
}

#[test]
fn curve_forwards_the_retained_fee_and_purchase_limit() {
    for rate in [0, 125, 9999] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool {
            sell_fee_bps,
            per_block_token_limit,
            ..
        } = &mut command
        {
            *sell_fee_bps = rate;
            *per_block_token_limit = Some(500);
        }
        input.message = borsh::to_vec(&command).unwrap();
        let mut mint = handle(&input).unwrap();
        for bytes in [config(), definition()] {
            input = reply(&input, &mint, bytes);
            mint = handle(&input).unwrap();
            assert_eq!(
                FactoryState::decode(mint.post_state.as_ref().unwrap())
                    .unwrap()
                    .sell_fee_bps,
                rate
            );
        }
        let (input, allocated) = continued(&input, &mint, 900, 0, 0);
        let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
        let state = FactoryState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.sell_fee_bps, rate);
        assert_eq!(state.per_block_token_limit, Some(500));
        let prepare = prepared
            .sync_messages
            .iter()
            .find(|call| call.to == Actor::new(input.receiver.account_id, state.curve_program_id))
            .unwrap();
        assert!(
            matches!(borsh::from_slice::<curve_core::Message>(&prepare.message).unwrap(), curve_core::Message::CreatePool { sell_fee_bps, per_block_token_limit: Some(500), .. } if sell_fee_bps == rate)
        );
        for case in 0..5 {
            let (mut request, response) = continued(&input, &prepared, 0, 800, 100);
            let mut saved = pool_state(&state, input.receiver, false);
            match case {
                0 => {}
                1 => saved.pool.sell_fee_bps = if rate == 0 { 1 } else { 0 },
                2 => {
                    saved.pool.k += 1;
                    saved.pool.virtual_reserve1 += 1;
                }
                3 => saved.pool.initial_k -= 1,
                _ => saved.block_allocation.limit = Some(501),
            }
            request = reply(&request, &response, borsh::to_vec(&saved).unwrap());
            let response = handle(&request).unwrap();
            request = reply(
                &request,
                &response,
                holding(state.pool_id, state.token_definition_id, 0),
            );
            let result = handle(&request);
            if case == 0 {
                assert_eq!(
                    FactoryState::decode(result.unwrap().post_state.as_ref().unwrap())
                        .unwrap()
                        .creation_stage,
                    CreationStage::Active
                );
            } else {
                assert!(result.is_err(), "rate {rate}, case {case}");
            }
        }
    }
}

#[test]
fn graduation_resolves_and_binds_the_current_creator_hint_before_custody_reads() {
    let (input, ready, saved) = hook_ready();
    let mut request = input.clone();
    request.pre_state = ready.post_state.clone().unwrap();
    request.from = None;
    for hint in [None, Some(AccountId::default())] {
        request.message =
            borsh::to_vec(&Message::ExecuteGraduationHook { creator_hint: hint }).unwrap();
        assert_eq!(handle(&request).err(), Some(Error::Unauthorized));
    }
    request.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    let resolving = handle(&request).unwrap();
    let valid = resolved(&request, &resolving, id(99));
    for (sender, role, creator) in [
        (
            Actor::new(id(99), AUTHORITY_PROGRAM_ACCOUNT_ID),
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            id(99),
        ),
        (
            Actor::new(
                compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
                id(99),
            ),
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            id(99),
        ),
        (
            Actor::new(
                compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
                AUTHORITY_PROGRAM_ACCOUNT_ID,
            ),
            id(99),
            id(99),
        ),
        (
            Actor::new(
                compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
                AUTHORITY_PROGRAM_ACCOUNT_ID,
            ),
            compute_curve_creator_role_pda(id(40), id(7), [3; 32]),
            id(42),
        ),
    ] {
        let mut forged = valid.clone();
        forged.from = Some(sender);
        forged.message = borsh::to_vec(&authority::CurrentAuthority {
            role,
            authority: creator,
        })
        .unwrap();
        assert_eq!(handle(&forged).err(), Some(Error::Unauthorized));
    }
    let (mut input, response) = settlement_as(&input, &ready, saved, 817, 70, true, id(99));
    input = reply(
        &input,
        &response,
        adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
    );
    let response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let transfers = token_calls(&response);
    assert!(transfers.contains(&token::Message::Transfer {
        definition: state.token_definition_id,
        recipient: id(99),
        amount: 600
    }));
    assert!(transfers.contains(&token::Message::Transfer {
        definition: state.collateral_definition_id,
        recipient: id(99),
        amount: 36
    }));
    assert!(!transfers.iter().any(
        |call| matches!(call, token::Message::Transfer { recipient, .. } if *recipient == id(42))
    ));
}

#[test]
fn curve_creation_rejects_invalid_shares_and_shares_without_an_adapter() {
    for (hook, launch_bps, collateral_bps) in [
        (false, 1, 0),
        (false, 0, 1),
        (true, 10001, 0),
        (true, 0, 10001),
    ] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool {
            graduation_hook_program_id,
            graduation_policy,
            ..
        } = &mut command
        {
            *graduation_hook_program_id = hook.then_some(id(9));
            *graduation_policy = GraduationPolicy {
                launch_bps,
                collateral_bps,
            };
        }
        input.message = borsh::to_vec(&command).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
    }
}

#[test]
fn curve_zero_balances_finalize_once_on_the_configured_route() {
    for hook in [false, true] {
        let (input, active) = active_for_route(hook);
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.settlement_stage = SettlementStage::Withdrawn {
            unsold: 0,
            collateral_due: 0,
        };
        let mut previous = active;
        previous.post_state = Some(borsh::to_vec(&state).unwrap().into());
        let mut saved = pool_state(&state, input.receiver, true);
        saved.pool.real_reserve0 = 0;
        saved.pool.real_reserve1 = 0;
        let (mut input, mut response) = settlement(&input, &previous, saved.clone(), 17, 19, hook);
        if hook {
            input = reply(
                &input,
                &response,
                adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
            );
            response = handle(&input).unwrap();
            let (next, pending) = settlement(&input, &response, saved, 17, 19, true);
            input = reply(
                &next,
                &pending,
                adapter_state(next.receiver, pending.post_state.as_ref().unwrap()),
            );
            response = handle(&input).unwrap();
            assert!(token_calls(&response).is_empty());
            let callback = response.sync_messages.last().unwrap();
            let AdapterMessage::Graduate { request } =
                borsh::from_slice(&callback.message).unwrap()
            else {
                panic!("graduate")
            };
            assert_eq!(
                request.intent,
                GraduationIntent::Curve {
                    launch_tokens: 0,
                    collateral: 0
                }
            );
            input.from = Some(callback.to);
            input.message = borsh::to_vec(&GraduationResult {
                factory: input.receiver.account_id,
                request_hash: request.hash(),
            })
            .unwrap();
            input.pre_state = response.post_state.unwrap();
            response = handle(&input).unwrap();
        }
        assert!(token_calls(&response).is_empty());
        let stage = FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage;
        assert_eq!(
            stage,
            if hook {
                SettlementStage::Graduated
            } else {
                SettlementStage::Complete
            }
        );
        input.pre_state = response.post_state.unwrap();
        input.from = None;
        input.message = borsh::to_vec(&if hook {
            Message::ExecuteGraduationHook {
                creator_hint: Some(id(42)),
            }
        } else {
            Message::WithdrawFactoryProceeds {
                creator: id(42),
                now: 400,
            }
        })
        .unwrap();
        assert_eq!(handle(&input).err(), Some(Error::AlreadySettled));
    }
}

#[test]
fn one_step_adapter_initialization_selects_curve_and_rejects_reinitialization() {
    let (input, response) = active_for_route(true);
    let factory = input.receiver;
    let state = adapter_state(factory, response.post_state.as_ref().unwrap());
    let decoded = AdapterState::decode(&state).unwrap();
    assert_eq!(decoded.accepted_intent, GraduationIntentKind::Curve);
    assert_eq!(decoded.factory, factory);
    assert!(decoded.receipt.is_none());
    let input = ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory.account_id), id(9)),
        from: None,
        is_authorized: false,
        pre_state: state.into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::FixedPrice,
        })
        .unwrap(),
    };
    assert_eq!(
        handle_adapter(&input).err(),
        Some(Error::AlreadyInitialized)
    );
}

#[test]
fn curve_payouts_use_the_withdrawal_snapshot_for_no_buyers_partial_sale_and_depletion() {
    for hook in [false, true] {
        for scenario in 0..3 {
            let (input, active) = active_for_route(hook);
            let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
            let mut saved = pool_state(&state, input.receiver, true);
            if scenario != 0 {
                saved
                    .pool
                    .swap_exact_input(pool::ExactInputSwap {
                        token_in: pool::TokenSide::Token1,
                        amount_in: 100,
                        min_amount_out: 1,
                        pool_fee_bps: 0,
                        protocol_fee_bps: 100,
                        now: 100,
                    })
                    .unwrap();
                assert!(saved.pool.fees_accrued > 0);
            }
            if scenario == 2 {
                saved.pool.real_reserve0 = 0;
            }
            let unsold = saved.pool.real_reserve0;
            let collateral = saved.pool.real_reserve1;
            let fees = saved.pool.fees_accrued;
            let (input, ready) = settlement(&input, &active, saved.clone(), 17, 19, hook);
            let (input, withdrawn) = settlement(&input, &ready, saved.clone(), 17, 19, hook);
            assert_eq!(
                FactoryState::decode(withdrawn.post_state.as_ref().unwrap())
                    .unwrap()
                    .settlement_stage,
                SettlementStage::Withdrawn {
                    unsold,
                    collateral_due: collateral
                }
            );
            saved.pool.real_reserve0 = 0;
            saved.pool.real_reserve1 = 0;
            let (mut input, mut response) = settlement(
                &input,
                &withdrawn,
                saved.clone(),
                unsold + 17,
                collateral + fees + 19,
                hook,
            );
            if hook {
                input = reply(
                    &input,
                    &response,
                    adapter_state(input.receiver, response.post_state.as_ref().unwrap()),
                );
                response = handle(&input).unwrap();
                let (next, pending) = settlement(
                    &input,
                    &response,
                    saved,
                    unsold + 17,
                    collateral + fees + 19,
                    true,
                );
                input = reply(
                    &next,
                    &pending,
                    adapter_state(next.receiver, pending.post_state.as_ref().unwrap()),
                );
                response = handle(&input).unwrap();
            }
            let calls = token_calls(&response);
            for (definition, eligible) in [
                (state.token_definition_id, unsold),
                (state.collateral_definition_id, collateral),
            ] {
                let paid: u128 = calls
                    .iter()
                    .filter_map(|call| match call {
                        token::Message::Transfer {
                            definition: actual,
                            amount,
                            ..
                        } if *actual == definition => Some(*amount),
                        _ => None,
                    })
                    .sum();
                assert_eq!(paid, eligible, "hook {hook}, scenario {scenario}");
            }
            assert!(
                calls
                    .iter()
                    .all(|call| matches!(call, token::Message::Transfer { .. }))
            );
            assert!(
                !FactoryState::decode(response.post_state.as_ref().unwrap())
                    .unwrap()
                    .creator_allocation_claimed
            );
        }
    }
}

#[test]
fn graduation_reserve_is_in_initial_supply_but_not_curve_pricing() {
    for reserve in [0, 120, u128::MAX] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool {
            graduation_reserve, ..
        } = &mut command
        {
            *graduation_reserve = reserve;
        }
        input.message = borsh::to_vec(&command).unwrap();
        let response = handle(&input);
        if reserve == u128::MAX {
            assert_eq!(response.err(), Some(Error::InvalidTerms));
        } else {
            let state =
                FactoryState::decode(response.unwrap().post_state.as_ref().unwrap()).unwrap();
            assert_eq!(state.graduation_reserve, reserve);
            assert_eq!(state.total_supply, 900 + reserve);
            assert_eq!(
                (
                    state.sale_reserve,
                    state.virtual_token_reserve,
                    state.virtual_collateral_reserve
                ),
                (800, 1000, 100)
            );
        }
    }
    assert_eq!(total_supply(1, u128::MAX - 2, 1), Ok(u128::MAX));
    assert_eq!(
        total_supply(1, u128::MAX - 2, 2),
        Err(CreateError::SupplyOverflow)
    );
}

#[test]
fn short_curve_sale_still_requires_inclusion_before_expiry() {
    for (now, accepted) in [
        (99, true),
        (100, true),
        (399, true),
        (400, false),
        (u64::MAX, false),
    ] {
        let mut input = creation();
        let mut message: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool { now: hint, .. } = &mut message {
            *hint = now;
        }
        input.message = borsh::to_vec(&message).unwrap();
        let result = handle(&input);
        if accepted {
            let response = result.unwrap();
            assert_eq!(response.validity.timestamps.end(), Some(400_000));
        } else {
            assert_eq!(result.err(), Some(Error::InvalidTerms));
        }
    }
    assert!(validate_curve_expiry(None, u64::MAX).is_ok());
}

#[test]
fn every_curve_setup_reply_keeps_the_expiry_window() {
    let mut input = creation();
    let mut response = handle(&input).unwrap();
    for bytes in [config(), definition()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
        assert_eq!(response.validity.timestamps.end(), Some(400_000));
    }
    let (input, allocated) = continued(&input, &response, 900, 0, 0);
    assert_eq!(allocated.validity.timestamps.end(), Some(400_000));
    let (input, prepared) = continued(&input, &allocated, 0, 800, 100);
    assert_eq!(prepared.validity.timestamps.end(), Some(400_000));
    let (_, activated) = continued(&input, &prepared, 0, 800, 100);
    assert_eq!(activated.validity.timestamps.end(), Some(400_000));
}

#[test]
fn curve_creation_rejects_invalid_retained_sell_fees_before_side_effects() {
    for rate in [10_000, u16::MAX] {
        let mut input = creation();
        let mut command: Message = borsh::from_slice(&input.message).unwrap();
        if let Message::CreateFactoryPool { sell_fee_bps, .. } = &mut command {
            *sell_fee_bps = rate;
        }
        input.message = borsh::to_vec(&command).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidTerms));
        let mut state = FactoryState::decode(active().1.post_state.as_ref().unwrap()).unwrap();
        state.sell_fee_bps = rate;
        assert!(state.validate(input.receiver).is_err());
    }
}
