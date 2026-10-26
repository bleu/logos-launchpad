use factory_core::*;
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{self, TokenKind, TokenState};
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
            creator_nft_definition: id(43),
            now: 100,
        })
        .unwrap(),
    }
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
            token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, creator, id(43)),
            TOKEN_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&token::NftHolder {
            definition: id(43),
            owner: creator,
        })
        .unwrap(),
    }
}
fn minted() -> (ReceiveInput, Response) {
    let mut input = creation();
    let mut response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
    response = handle(&input).unwrap();
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
    let (_, response) = continued(&input, &allocated, 0, 900, 100);
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.creation_stage, CreationStage::PoolPrepared);
    let call = response
        .sync_messages
        .iter()
        .find(|m| m.to == Actor::new(state.pool_id, id(6)))
        .unwrap();
    let curve_core::Message::CreatePool {
        allowed_input_token,
        defer_funding,
        token0_amount,
        token1_amount,
        owner_program_id,
        close_on_depletion,
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
}
fn pool_state(state: &FactoryState, owner: Actor, funded: bool) -> curve_core::PoolAccount {
    curve_core::PoolAccount {
        allowed_input_token: Some(pool::TokenSide::Token1),
        funded,
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
            close_timestamp: Some(200),
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap(),
    }
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
        Message::ExecuteGraduationHook { now: 200 }
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
    let clock = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 200_000,
            at_most: u64::MAX
        }
    );
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
fn creator_allocation_follows_the_nft_to_its_current_holder_after_closure() {
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
        Actor::new(id(99), TOKEN_PROGRAM_ACCOUNT_ID)
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
            ready,
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
                dex_seed_reserve: 100,
                collateral_amount: 50,
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
    input.pre_state = response.post_state.unwrap();
    input.message = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
    })
    .unwrap();
    input.from = Some(Actor::new(custody, id(99)));
    assert!(matches!(handle(&input), Err(Error::Unauthorized)));
    input.from = Some(Actor::new(custody, id(9)));
    let complete = handle(&input).unwrap();
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
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook { now: 200 }).unwrap();
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
    assert_eq!(TradeMode::try_from(2), Err(CreateError::InvalidTradeMode));
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
        compute_creator_commitment(id(40), id(43), [3; 32]),
        compute_creator_commitment(id(40), id(44), [3; 32])
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
    let clock = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 200_000,
            at_most: u64::MAX
        }
    );
}

#[test]
fn zero_manual_payout_keeps_one_saved_zero_hook_available() {
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
    let response = handle(&input).unwrap();
    input = proof(&input, &response, id(42));
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
    let (input, no_payout) = settlement(&input, &burned, saved.clone(), 0, 0, false);
    assert_eq!(
        FactoryState::decode(no_payout.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_stage,
        SettlementStage::NoPayout
    );
    let (mut input, mut response) = settlement(&input, &no_payout, saved.clone(), 0, 0, true);
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
                dex_seed_reserve: 0,
                collateral_amount: 0,
                ..
            }
        }
    ));
    input.pre_state = response.post_state.unwrap();
    input.from = Some(response.sync_messages[0].to);
    input.message = borsh::to_vec(&GraduationResult {
        factory: input.receiver.account_id,
    })
    .unwrap();
    let complete = handle(&input).unwrap();
    input.pre_state = complete.post_state.unwrap();
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook { now: 200 }).unwrap();
    assert!(matches!(handle(&input), Err(Error::AlreadySettled)));
}

#[test]
fn curve_creation_authenticates_nft_creator_and_mints_the_three_way_supply() {
    let input = creation();
    let response = handle(&input).unwrap();
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(id(42), TOKEN_PROGRAM_ACCOUNT_ID)
    );
    let (_, response) = minted();
    let state = FactoryState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.total_supply, 1000);
    assert_eq!(
        state.creator_commitment,
        compute_creator_commitment(id(40), id(43), [3; 32])
    );
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
