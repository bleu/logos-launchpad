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
            graduation_hook_program_id: Some(id(9)),
            graduation_hook_configuration: b"route".to_vec(),
            namespace: id(40),
            launch_salt: [3; 32],
            name: "Launch".into(),
            uri: "uri".into(),
            sale_reserve: 800,
            dex_seed_reserve: 100,
            creator_allocation: 100,
            virtual_token_reserve: 1000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(200),
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
fn minted() -> (ReceiveInput, Response) {
    minted_with_policy(curve_core::BuyPolicy::Open)
}
fn minted_with_policy(policy: curve_core::BuyPolicy) -> (ReceiveInput, Response) {
    let mut input = creation();
    let mut message: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool { buy_policy, .. } = &mut message {
        *buy_policy = policy;
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
fn curve_allocation_reserves_d_plus_r_and_locks_creator_tokens() {
    let (input, mint) = minted();
    let (input, response) = continued(&input, &mint, 1000, 0, 0);
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
                amount: 900
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
    let (input, allocated) = continued(&input, &mint, 1000, 0, 0);
    let (input, response) = continued(&input, &allocated, 0, 900, 100);
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
        allowed_input_token: Some(pool::TokenSide::Token1),
        funded,
        closure_observed: false,
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
    let (input, allocated) = continued(&input, &mint, 1000, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 900, 100);
    let (input, response) = continued(&input, &prepared, 0, 900, 100);
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
    let (input, allocated) = continued(&input, &mint, 1000, 0, 0);
    assert_eq!(
        FactoryState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let (input, prepared) = continued(&input, &allocated, 0, 900, 100);
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
    let (mut input, mut response) = continued(&input, &prepared, 0, 900, 100);
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
    let (input, mint) = minted();
    let (input, allocated) = continued(&input, &mint, 1000, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 900, 100);
    let (mut input, mut response) = continued(&input, &prepared, 0, 900, 100);
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
    let state = FactoryState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&if hook {
        Message::ExecuteGraduationHook { creator_hint: None }
    } else {
        Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 200,
        }
    })
    .unwrap();
    let mut response = handle(&input).unwrap();
    if !hook {
        input = proof(&input, &response, id(42));
        response = handle(&input).unwrap()
    }
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
            (pool::PoolLifecycle::Open, Some(200_000)),
            (pool::PoolLifecycle::Closed, None),
        ] {
            let (input, active) = active();
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
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
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
fn withdrawn() -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let saved = pool_state(&state, input.receiver, true);
    let (input, ready) = settlement(&input, &active, saved.clone(), 100, 0, false);
    let (input, response) = settlement(&input, &ready, saved.clone(), 100, 0, false);
    (input, response, saved)
}
#[test]
fn reserve_withdrawal_records_unsold_d_and_recorded_collateral_then_uses_atomic_curve_withdrawal() {
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
        curve_core::Message::WithdrawReserves { now: 200 }
    );
    assert_eq!(state.dex_seed_reserve, 100);
    assert_eq!(state.creator_allocation, 100);
    assert_eq!(
        input.receiver.account_id,
        compute_factory_pda(id(40), id(7), [3; 32])
    );
}
#[test]
fn expiry_burns_unsold_d_once_without_burning_r_or_creator_allocation() {
    let (input, withdrawn, mut saved) = withdrawn();
    saved.pool.real_reserve0 = 0;
    saved.pool.real_reserve1 = 0;
    let (_, response) = settlement(&input, &withdrawn, saved, 900, 17, false);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::Burned { collateral_due: 0 }
    );
    let burn = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(input.receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<token::Message>(&burn.message).unwrap(),
        token::Message::Burn {
            definition: state.token_definition_id,
            amount: 800
        }
    );
    assert_eq!(state.dex_seed_reserve, 100);
    assert!(!state.creator_allocation_claimed);
}
fn burned(collateral: u128) -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, true);
    saved.pool.real_reserve1 = collateral;
    let (input, ready) = settlement(&input, &active, saved.clone(), 100, 0, false);
    let (input, withdrawn) = settlement(&input, &ready, saved.clone(), 100, 0, false);
    saved.pool.real_reserve0 = 0;
    saved.pool.real_reserve1 = 0;
    let (input, response) = settlement(
        &input,
        &withdrawn,
        saved.clone(),
        900,
        collateral + 17,
        false,
    );
    (input, response, saved)
}
#[test]
fn manual_settlement_pays_only_r_then_recorded_collateral_and_rejects_replay() {
    let (input, burned, saved) = burned(50);
    let (input, paid) = settlement(&input, &burned, saved.clone(), 100, 67, false);
    let state = FactoryState::decode(paid.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::TokenPaid { collateral_due: 50 }
    );
    let payout = paid
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(input.receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<token::Message>(&payout.message).unwrap(),
        token::Message::Transfer {
            definition: state.token_definition_id,
            recipient: id(42),
            amount: 100
        }
    );
    let (input, complete) = settlement(&input, &paid, saved.clone(), 0, 67, false);
    let state = FactoryState::decode(complete.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.settlement_stage, SettlementStage::Complete);
    let payout = complete
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(input.receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID))
        .unwrap();
    assert_eq!(
        borsh::from_slice::<token::Message>(&payout.message).unwrap(),
        token::Message::Transfer {
            definition: state.collateral_definition_id,
            recipient: id(42),
            amount: 50
        }
    );
    let mut input = input.clone();
    input.pre_state = complete.post_state.unwrap();
    input.message = borsh::to_vec(&Message::WithdrawFactoryProceeds {
        creator: id(42),
        now: 200,
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::AlreadySettled)));
}
#[test]
fn allocation_uses_saved_closure_instead_of_a_future_client_time() {
    for (lifecycle, expected) in [
        (pool::PoolLifecycle::Open, Some(200_000)),
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
        now: 200,
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
        now: 200,
    })
    .unwrap();
    assert!(matches!(handle(&input), Err(Error::AlreadyClaimed)));
}
fn adapter_state(factory: Actor, ready: bool) -> Vec<u8> {
    handle_adapter(&ReceiveInput {
        receiver: Actor::new(compute_graduation_pda(id(9), factory.account_id), id(9)),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: ready.then_some(GraduationIntentKind::CurveReserves),
        })
        .unwrap(),
    })
    .unwrap()
    .post_state
    .unwrap()
    .to_vec()
}
fn hook_ready() -> (ReceiveInput, Response, curve_core::PoolAccount) {
    let (input, burned, saved) = burned(50);
    let (mut input, mut response) = settlement(&input, &burned, saved.clone(), 100, 67, true);
    input = reply(&input, &response, adapter_state(input.receiver, true));
    response = handle(&input).unwrap();
    (input, response, saved)
}
#[test]
fn hook_preparation_requires_saved_adapter_custody_and_keeps_manual_recovery() {
    let (input, response, saved) = hook_ready();
    assert!(
        matches!(&launch_events(&response)[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Prepared, route: None, effects }, .. }] if effects.is_empty())
    );
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady { collateral_due: 50 }
    );
    let (_, manual) = settlement(&input, &response, saved, 100, 67, false);
    assert_eq!(
        FactoryState::decode(manual.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::TokenPaid { collateral_due: 50 }
    );
}
#[test]
fn hook_finalization_pays_custody_without_factory_grant_and_waits_for_authenticated_success() {
    let (input, ready, saved) = hook_ready();
    let (mut input, mut response) = settlement(&input, &ready, saved, 100, 67, true);
    input = reply(&input, &response, adapter_state(input.receiver, true));
    response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady { collateral_due: 50 }
    );
    assert!(
        matches!(&launch_events(&response)[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Payout, route: Some(SettlementRouteV1::Graduation), effects }, .. }] if matches!(&effects[..], [SettlementEffectV1::Payout { purpose: PayoutPurposeV1::CurveReserve, liability_discharged: 100, tokens_transferred: 100, .. }, SettlementEffectV1::Payout { purpose: PayoutPurposeV1::CreatorProceeds, liability_discharged: 50, tokens_transferred: 50, .. }]))
    );
    let custody = compute_graduation_pda(id(9), input.receiver.account_id);
    let callback = response.sync_messages.last().unwrap();
    assert_eq!(callback.to, Actor::new(custody, id(9)));
    assert!(callback.pda_seeds.is_empty());
    assert_eq!(
        borsh::from_slice::<AdapterMessage>(&callback.message).unwrap(),
        AdapterMessage::Graduate {
            request: GraduationRequest {
                factory_program_id: id(7),
                namespace: id(40),
                launch_salt: [3; 32],
                token_definition_id: state.token_definition_id,
                collateral_definition_id: state.collateral_definition_id,
                intent: GraduationIntent::CurveReserves {
                    launch_tokens: 100,
                    collateral: 50
                },
                configuration: b"route".to_vec()
            }
        }
    );
    let payouts: Vec<_> = response
        .sync_messages
        .iter()
        .filter(|m| m.to == Actor::new(input.receiver.account_id, TOKEN_PROGRAM_ACCOUNT_ID))
        .collect();
    assert_eq!(payouts.len(), 2);
    assert!(
        payouts
            .iter()
            .all(|m| m.pda_seeds.contains(&compute_factory_seed(id(40), [3; 32])))
    );
    let AdapterMessage::Graduate { request } =
        borsh::from_slice(&response.sync_messages.last().unwrap().message).unwrap()
    else {
        panic!("graduate");
    };
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
        request_hash: request.hash(),
    })
    .unwrap();
    input.from = Some(Actor::new(custody, id(99)));
    assert!(matches!(handle(&input), Err(Error::Unauthorized)));
    input.from = Some(Actor::new(custody, id(9)));
    let valid = input.message.clone();
    input.message = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
        request_hash: [0; 32],
    })
    .unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    input.message = borsh::to_vec(&Message::ReadState).unwrap();
    assert_eq!(handle(&input).err(), Some(Error::InvalidMessage));
    input.message = valid;
    let complete = handle(&input).unwrap();
    let events = launch_events(&complete);
    assert!(matches!(
        &events[..],
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
                    launch_tokens: 100,
                    collateral: 50,
                    ..
                },
                ..
            }
        ]
    ));
    assert_eq!(
        FactoryState::decode(complete.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Graduated
    );
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
        now: 200,
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
fn manual_r_payout_with_zero_collateral_is_complete_and_cannot_reopen_the_hook() {
    let (input, burned, saved) = burned(0);
    let (input, paid) = settlement(&input, &burned, saved.clone(), 100, 17, false);
    let (mut input, complete) = settlement(&input, &paid, saved, 0, 17, false);
    assert_eq!(
        FactoryState::decode(complete.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Complete
    );
    input.pre_state = complete.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook { creator_hint: None }).unwrap();
    assert!(matches!(handle(&input), Err(Error::ManualPayoutStarted)));
}
#[test]
fn public_supply_and_address_rules_preserve_the_original_factory_contract() {
    assert_eq!(total_supply(800, 100, 100), Ok(1000));
    assert_eq!(total_supply(0, 100, 100), Err(CreateError::SaleReserveZero));
    assert_eq!(
        total_supply(u128::MAX, 1, 0),
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
    assert_eq!(response.validity.timestamps.start(), Some(200_000));
    assert_eq!(response.validity.timestamps.end(), None);
}

#[test]
fn zero_manual_completion_blocks_hooks_but_a_zero_hook_can_finish_first() {
    let mut input = creation();
    let mut command: Message = borsh::from_slice(&input.message).unwrap();
    if let Message::CreateFactoryPool {
        dex_seed_reserve,
        creator_allocation,
        ..
    } = &mut command
    {
        *dex_seed_reserve = 0;
        *creator_allocation = 200
    }
    input.message = borsh::to_vec(&command).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [config(), definition()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    let (input, allocated) = continued(&input, &response, 1000, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 800, 200);
    let (mut input, mut response) = continued(&input, &prepared, 0, 800, 200);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    let mut saved = pool_state(&state, input.receiver, false);
    for bytes in [
        borsh::to_vec(&saved).unwrap(),
        holding(state.pool_id, state.token_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap()
    }
    saved.funded = true;
    let (input, ready) = settlement(&input, &response, saved.clone(), 0, 0, false);
    let (input, withdrawn) = settlement(&input, &ready, saved.clone(), 0, 0, false);
    saved.pool.real_reserve0 = 0;
    let (input, burned) = settlement(&input, &withdrawn, saved.clone(), 800, 0, false);
    let (mut manual_input, complete) = settlement(&input, &burned, saved.clone(), 0, 0, false);
    let events = launch_events(&complete);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Completed, route: Some(SettlementRouteV1::Manual), effects }, .. }] if effects.is_empty())
    );
    assert_eq!(
        FactoryState::decode(complete.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::Complete
    );
    assert!(complete.sync_messages.is_empty());
    manual_input.pre_state = complete.post_state.unwrap();
    manual_input.from = None;
    manual_input.message =
        borsh::to_vec(&Message::ExecuteGraduationHook { creator_hint: None }).unwrap();
    assert!(matches!(
        handle(&manual_input),
        Err(Error::ManualPayoutStarted)
    ));
    let (mut input, mut response) = settlement(&input, &burned, saved.clone(), 0, 0, true);
    input = reply(&input, &response, adapter_state(input.receiver, true));
    response = handle(&input).unwrap();
    let (mut input, mut response) = settlement(&input, &response, saved, 0, 0, true);
    input = reply(&input, &response, adapter_state(input.receiver, true));
    response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert!(matches!(
        borsh::from_slice::<AdapterMessage>(&response.sync_messages[0].message).unwrap(),
        AdapterMessage::Graduate {
            request: GraduationRequest {
                intent: GraduationIntent::CurveReserves {
                    launch_tokens: 0,
                    collateral: 0
                },
                ..
            }
        }
    ));
    let AdapterMessage::Graduate { request } =
        borsh::from_slice(&response.sync_messages[0].message).unwrap()
    else {
        panic!("graduate");
    };
    input.pre_state = response.post_state.unwrap();
    input.from = Some(response.sync_messages[0].to);
    input.message = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
        request_hash: request.hash(),
    })
    .unwrap();
    let complete = handle(&input).unwrap();
    let events = launch_events(&complete);
    assert!(matches!(
        &events[..],
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
                    launch_tokens: 0,
                    collateral: 0,
                    ..
                },
                ..
            }
        ]
    ));
    let progress = FactoryState::decode(complete.post_state.as_ref().unwrap())
        .unwrap()
        .settlement_stage
        .progress();
    assert_eq!(progress, SettlementProgress::Graduated);
    assert_eq!(progress.route(), Some(SettlementRoute::Graduation));
    input.pre_state = complete.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook { creator_hint: None }).unwrap();
    assert!(matches!(handle(&input), Err(Error::AlreadySettled)));
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
    assert_eq!(response.validity.timestamps.end(), Some(200_000));
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
    assert_eq!(state.total_supply, 1000);
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
    assert_eq!(supply, 1000);
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
                now: 200,
            },
        ),
        (
            CreationStage::Active,
            SettlementStage::Burned { collateral_due: 50 },
            Message::WithdrawFactoryProceeds {
                creator: id(99),
                now: 200,
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
                current.dex_seed_reserve,
                current.creator_allocation,
                current.total_supply
            ),
            (800, 100, 100, 1000)
        );
        assert_eq!(
            (
                current.virtual_token_reserve,
                current.virtual_collateral_reserve,
                current.end_timestamp
            ),
            (1000, 100, Some(200))
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
            now: 200,
        },
        Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 200,
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
        let (input, allocated) = continued(&input, &mint, 1000, 0, 0);
        assert_eq!(
            FactoryState::decode(allocated.post_state.as_ref().unwrap())
                .unwrap()
                .pause_policy,
            policy
        );
        let (input, prepared) = continued(&input, &allocated, 0, 900, 100);
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
            let (mut input, mut response) = continued(&input, &prepared, 0, 900, 100);
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
        for end in [None, Some(200)] {
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
    let (input, allocated) = continued(&input, &mint, 1000, 0, 0);
    let (input, prepared) = continued(&input, &allocated, 0, 900, 100);
    let (mut input, mut response) = continued(&input, &prepared, 0, 900, 100);
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
                let (mut input, active) = active();
                let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
                state.ending_policy = ending;
                state.end_timestamp = if ending == EndingPolicy::Depletion {
                    None
                } else {
                    Some(200)
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
                    _ => Message::ExecuteGraduationHook { creator_hint: None },
                };
                input.message = borsh::to_vec(&command).unwrap();
                let mut response = handle(&input).unwrap();
                if action != 2 {
                    input = proof(&input, &response, id(42));
                    response = handle(&input).unwrap();
                }
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
                    if early { None } else { Some(200_000) }
                );
                assert_eq!(response.validity.timestamps.end(), None);
                assert_eq!(response.validity.timestamps.is_valid_for(199_999), early);
                assert!(response.validity.timestamps.is_valid_for(200_000));
                let after = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
                assert_eq!(after.dex_seed_reserve, 100);
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
            Some(200)
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
        let (input, allocated) = continued(&input, &response, 1000, 0, 0);
        let (input, prepared) = continued(&input, &allocated, 0, 900, 100);
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
        let (mut input, mut response) = continued(&input, &prepared, 0, 900, 100);
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
        assert_eq!(active.dex_seed_reserve, 100);
        assert_eq!(active.creator_allocation, 100);
    }
}

#[test]
fn curve_settlement_progress_projects_existing_stages_without_changing_wire_tags() {
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
            SettlementStage::Burned { collateral_due: 29 },
            SettlementProgress::Unselected,
            3,
            vec![29],
        ),
        (
            SettlementStage::TokenPaid { collateral_due: 29 },
            SettlementProgress::ManualStarted,
            4,
            vec![29],
        ),
        (
            SettlementStage::Complete,
            SettlementProgress::ManualComplete,
            5,
            vec![],
        ),
        (
            SettlementStage::HookReady { collateral_due: 29 },
            SettlementProgress::Unselected,
            6,
            vec![29],
        ),
        (
            SettlementStage::Graduated,
            SettlementProgress::Graduated,
            7,
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
fn curve_settlement_entrypoints_keep_route_errors_and_hook_configuration_precedence() {
    let (input, active) = active();
    let original = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    for (stage, manual_error, hook_error) in [
        (SettlementStage::Unstarted, None, None),
        (SettlementStage::Ready, None, None),
        (
            SettlementStage::Withdrawn {
                unsold: 17,
                collateral_due: 29,
            },
            None,
            None,
        ),
        (SettlementStage::Burned { collateral_due: 29 }, None, None),
        (
            SettlementStage::HookReady { collateral_due: 29 },
            None,
            None,
        ),
        (
            SettlementStage::TokenPaid { collateral_due: 29 },
            None,
            Some(Error::ManualPayoutStarted),
        ),
        (
            SettlementStage::Complete,
            Some(Error::AlreadySettled),
            Some(Error::ManualPayoutStarted),
        ),
        (
            SettlementStage::Graduated,
            Some(Error::AlreadySettled),
            Some(Error::AlreadySettled),
        ),
    ] {
        for hook in [false, true] {
            let mut state = original.clone();
            state.settlement_stage = stage;
            let mut request = input.clone();
            request.pre_state = borsh::to_vec(&state).unwrap().into();
            request.from = None;
            request.message = borsh::to_vec(&if hook {
                Message::ExecuteGraduationHook { creator_hint: None }
            } else {
                Message::WithdrawFactoryProceeds {
                    creator: id(42),
                    now: 200,
                }
            })
            .unwrap();
            let result = handle(&request);
            if let Some(error) = if hook { hook_error } else { manual_error } {
                assert_eq!(result.err(), Some(error));
            } else {
                let response = result.unwrap();
                assert_eq!(
                    FactoryState::decode(response.post_state.as_ref().unwrap())
                        .unwrap()
                        .settlement_stage,
                    stage
                );
                if !hook {
                    assert_eq!(
                        response.sync_messages.last().unwrap().to,
                        Actor::new(id(42), AUTHORITY_PROGRAM_ACCOUNT_ID)
                    );
                }
            }
            if hook {
                state.graduation_hook = None;
                request.pre_state = borsh::to_vec(&state).unwrap().into();
                assert_eq!(handle(&request).err(), Some(Error::HookNotConfigured));
            }
        }
    }
}

#[test]
fn curve_preparation_leaves_both_routes_available_until_a_manual_asset_payout() {
    let (input, active) = active();
    let state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
    let mut pool = pool_state(&state, input.receiver, true);
    pool.pool.real_reserve1 = 50;
    let (input, ready) = settlement(&input, &active, pool.clone(), 100, 0, false);
    let (input, withdrawn) = settlement(&input, &ready, pool.clone(), 100, 0, false);
    pool.pool.real_reserve0 = 0;
    pool.pool.real_reserve1 = 0;
    let (input, burned) = settlement(&input, &withdrawn, pool.clone(), 900, 50, false);
    for response in [&active, &ready, &withdrawn, &burned] {
        let progress = FactoryState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage
            .progress();
        assert_eq!(progress, SettlementProgress::Unselected);
        assert_eq!(progress.route(), None);
        assert!(progress.ensure_available(SettlementRoute::Manual).is_ok());
        assert!(
            progress
                .ensure_available(SettlementRoute::Graduation)
                .is_ok()
        );
    }
    let (mut input, mut response) = settlement(&input, &burned, pool.clone(), 100, 50, true);
    input = reply(&input, &response, adapter_state(input.receiver, true));
    response = handle(&input).unwrap();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage,
        SettlementStage::HookReady { collateral_due: 50 }
    );
    assert_eq!(
        state.settlement_stage.progress(),
        SettlementProgress::Unselected
    );
    let (input, paid) = settlement(&input, &response, pool.clone(), 100, 50, false);
    let state = FactoryState::decode(paid.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage.progress(),
        SettlementProgress::ManualStarted
    );
    assert_eq!(
        state.settlement_stage.progress().route(),
        Some(SettlementRoute::Manual)
    );
    let (_, complete) = settlement(&input, &paid, pool, 0, 50, false);
    let state = FactoryState::decode(complete.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_stage.progress(),
        SettlementProgress::ManualComplete
    );
    assert!(state.settlement_stage.is_manual_settlement_complete());
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
                sale_inventory: 800,
                creator_allocation: 100,
                curve_reserve: 100,
                collateral_seed: 0,
                start_timestamp: None,
                end_timestamp: Some(200),
                policies: EventPoliciesV1 {
                    ending: EndingPolicy::Either,
                    pause: PausePolicy::Disabled
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
fn curve_settlement_events_report_factory_burn_payouts_and_final_route() {
    let (input, burned, pool) = burned(50);
    let events = launch_events(&burned);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::UnsoldBurned, route: None, effects }, .. }] if effects == &vec![SettlementEffectV1::Burn { definition: *FactoryState::decode(burned.post_state.as_ref().unwrap()).unwrap().token_definition_id.value(), amount: 800 }])
    );
    let (input, paid) = settlement(&input, &burned, pool.clone(), 100, 67, false);
    let events = launch_events(&paid);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Payout, route: Some(SettlementRouteV1::Manual), effects }, .. }] if matches!(&effects[..], [SettlementEffectV1::Payout { purpose: PayoutPurposeV1::CurveReserve, liability_discharged: 100, tokens_transferred: 100, .. }]))
    );
    let (_, complete) = settlement(&input, &paid, pool, 0, 67, false);
    let events = launch_events(&complete);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Payout, route: Some(SettlementRouteV1::Manual), effects }, .. }, SaleEventV1 { kind: SaleEventKindV1::Settlement { step: SettlementStepV1::Completed, route: Some(SettlementRouteV1::Manual), effects: completed }, .. }] if matches!(&effects[..], [SettlementEffectV1::Payout { purpose: PayoutPurposeV1::CreatorProceeds, liability_discharged: 50, tokens_transferred: 50, .. }]) && completed.is_empty())
    );
    let (_, withdrawn, _) = withdrawn();
    assert!(withdrawn.events.is_empty());
}

#[test]
fn curve_allocation_read_emits_only_actual_allocation_payment() {
    for (amount, self_recipient) in [(0, false), (100, false), (100, true)] {
        let (mut input, active) = active();
        let mut state = FactoryState::decode(active.post_state.as_ref().unwrap()).unwrap();
        state.creator_allocation = amount;
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
        assert_eq!(response.validity.timestamps.start(), Some(200_000));
    }
}

#[test]
fn curve_manual_self_recipient_reports_discharged_claim_without_token_movement() {
    let (mut input, burned, pool) = burned(0);
    let state = FactoryState::decode(burned.post_state.as_ref().unwrap()).unwrap();
    input.pre_state = burned.post_state.unwrap();
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
        holding(input.receiver.account_id, state.token_definition_id, 100),
        holding(input.receiver.account_id, state.collateral_definition_id, 0),
    ] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    let events = launch_events(&response);
    assert!(
        matches!(&events[..], [SaleEventV1 { kind: SaleEventKindV1::Settlement { effects, .. }, .. }] if matches!(&effects[..], [SettlementEffectV1::Payout { liability_discharged: 100, tokens_transferred: 0, .. }]))
    );
}
