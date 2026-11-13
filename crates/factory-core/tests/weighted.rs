#[path = "support/lbp_pause_forwarding.rs"]
mod lbp_pause_forwarding;

use factory_core::events::*;
use factory_core::{
    CreatorAllocationRelease, Error, LbpCreationStage, LbpLaunchState, LbpSetupMode, Message,
    WeightedInputs, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response, StateReply},
};
use lez_compat_core::token::{TokenKind, TokenState};

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn inputs() -> WeightedInputs {
    WeightedInputs {
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::AuthorityControlled,
        ending_policy: factory_core::EndingPolicy::Scheduled,
        trade_mode: factory_core::TradeMode::BuyOnly,
        graduation_hook: None,
        namespace: id(40),
        launch_salt: [3; 32],
        name: "Weighted".into(),
        uri: "https://example.test/weighted".into(),
        sale_inventory: 800,
        creator_allocation: 200,
        collateral_seed: 100,
        token_weight_start: 95 * lbp_core::WEIGHT_TOTAL / 100,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
        start_timestamp: 150,
        end_timestamp: 300,
        allocation_release: CreatorAllocationRelease::AfterEnd,
        namespace_program_id: id(6),
        lbp_program_id: id(8),
        collateral_definition_id: id(41),
        creator: id(42),
    }
}
fn creation(value: WeightedInputs) -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(
            factory_core::compute_factory_pda(value.namespace, id(7), value.launch_salt),
            id(7),
        ),
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::CreateWeightedLbp { inputs: value }).unwrap(),
    }
}
fn config() -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: true,
        pre_state: vec![].into(),
        message: borsh::to_vec(&curve_core::Message::InitializeNamespace {
            namespace: id(40),
            protocol_fee_bps: 100,
            treasury: id(43),
        })
        .unwrap(),
    };
    curve_core::handle(&input)
        .unwrap()
        .post_state
        .unwrap()
        .to_vec()
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
fn definition() -> Vec<u8> {
    borsh::to_vec(&TokenState::Definition {
        name: inputs().name,
        kind: TokenKind::Fungible,
        supply: 1000,
        metadata: Some(lez_compat_core::token::Metadata {
            standard: lez_compat_core::token::MetadataStandard::Simple,
            uri: inputs().uri,
            creators: String::new(),
        }),
    })
    .unwrap()
}
fn reply(input: &ReceiveInput, response: &Response, value: Vec<u8>) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&StateReply {
            state: value.into(),
        })
        .unwrap(),
    }
}
fn minted(release: CreatorAllocationRelease) -> (ReceiveInput, Response) {
    let mut value = inputs();
    value.allocation_release = release;
    minted_with_terms(value)
}
fn minted_with_terms(value: WeightedInputs) -> (ReceiveInput, Response) {
    let mut input = creation(value);
    let mut response = handle(&input).unwrap();
    for bytes in [config(), definition()] {
        input = reply(&input, &response, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}
fn advance(
    input: &ReceiveInput,
    previous: &Response,
    extra: Vec<Vec<u8>>,
) -> Result<Response, Error> {
    let state = LbpLaunchState::decode(previous.post_state.as_ref().unwrap()).unwrap();
    let mut input = input.clone();
    input.pre_state = previous.post_state.clone().unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::ContinueWeightedCreation {
        expected_stage: state.creation_stage,
        creator: id(42),
    })
    .unwrap();
    let response = handle(&input)?;
    input.pre_state = response.post_state.unwrap();
    input.from = Some(Actor::new(
        state.creator_role_id,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    input.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
        role: state.creator_role_id,
        authority: id(42),
    })
    .unwrap();
    let mut response = handle(&input)?;
    let mint = factory_core::compute_mint_pda(
        state.namespace,
        input.receiver.program_account_id,
        state.launch_salt,
    );
    let first = state.creation_stage == LbpCreationStage::Minted;
    let mut values = vec![
        definition(),
        holding(
            mint,
            state.token_definition_id,
            if first { 1000 } else { 0 },
        ),
        holding(
            input.receiver.account_id,
            state.token_definition_id,
            if first { 0 } else { 800 },
        ),
        holding(
            state.creator_escrow_id,
            state.token_definition_id,
            if first { 0 } else { 200 },
        ),
    ];
    values.extend(extra);
    for value in values {
        input = reply(&input, &response, value);
        response = handle(&input)?;
    }
    Ok(response)
}
fn prepared(release: CreatorAllocationRelease) -> (ReceiveInput, Response, Vec<u8>) {
    let mut terms = inputs();
    terms.allocation_release = release;
    prepared_with_terms(terms)
}
fn prepared_with_terms(terms: WeightedInputs) -> (ReceiveInput, Response, Vec<u8>) {
    let policy = terms.buy_policy.clone();
    let pause_policy = terms.pause_policy;
    let policies = terms.ending_policy;
    let trade_mode = terms.trade_mode;
    let (input, response) = minted_with_terms(terms);
    assert_eq!(
        LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .pause_policy,
        pause_policy
    );
    assert_eq!(
        LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let allocated = advance(&input, &response, vec![]).unwrap();
    assert_eq!(
        LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .pause_policy,
        pause_policy
    );
    assert_eq!(
        LbpLaunchState::decode(allocated.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    let prepared = advance(&input, &allocated, vec![]).unwrap();
    assert_eq!(
        LbpLaunchState::decode(prepared.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        policy
    );
    for response in [&response, &allocated, &prepared] {
        let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.ending_policy, policies);
        assert_eq!(state.trade_mode, trade_mode);
    }
    let call = prepared
        .sync_messages
        .iter()
        .find(|m| m.to.program_account_id == id(8))
        .unwrap();
    let mut pool_input = ReceiveInput {
        receiver: call.to,
        from: Some(input.receiver),
        is_authorized: false,
        pre_state: vec![].into(),
        message: call.message.clone(),
    };
    let mut pool = lbp_core::handle(&pool_input).unwrap();
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    for value in [
        config(),
        definition(),
        definition(),
        holding(input.receiver.account_id, state.token_definition_id, 800),
        holding(state.pool_id, state.token_definition_id, 0),
        holding(state.pool_id, state.collateral_definition_id, 0),
    ] {
        pool_input = reply(&pool_input, &pool, value);
        pool = lbp_core::handle(&pool_input).unwrap();
    }
    (input, prepared, pool.post_state.unwrap().to_vec())
}

#[test]
fn weighted_creation_validates_terms_and_keeps_a_separate_mode() {
    for case in 0..9 {
        let mut value = inputs();
        match case {
            0 => value.collateral_seed = 0,
            1 => value.token_weight_start = lbp_core::MIN_WEIGHT - 1,
            2 => value.token_weight_end = lbp_core::WEIGHT_TOTAL - lbp_core::MIN_WEIGHT + 1,
            3 => value.end_timestamp = value.start_timestamp,
            4 => value.creator = AccountId::default(),
            5 => value.name.clear(),
            6 => value.uri.clear(),
            7 => value.creator_allocation = u128::MAX,
            _ => {
                value.collateral_definition_id =
                    factory_core::compute_definition_pda(value.namespace, id(7), value.launch_salt)
            }
        }
        assert_eq!(
            handle(&creation(value)).err(),
            Some(if case == 7 {
                Error::Overflow
            } else {
                Error::InvalidTerms
            })
        );
    }
    let (input, response) = minted(CreatorAllocationRelease::AfterEnd);
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(matches!(
        state.mode,
        LbpSetupMode::Weighted {
            collateral_seed: 100,
            ..
        }
    ));
    assert_eq!(state.total_supply, 1000);
    let clock = response
        .sync_messages
        .iter()
        .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
        .unwrap();
    assert_eq!(
        borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
        clock_core::Message::AssertTimestamp {
            at_least: 0,
            at_most: 150_000
        }
    );
    let mut request = input.clone();
    request.pre_state = response.post_state.unwrap();
    request.message = borsh::to_vec(&Message::ContinueFixedPriceCreation {
        expected_stage: LbpCreationStage::Minted,
        creator: id(42),
        now: 100,
    })
    .unwrap();
    assert_eq!(handle(&request).err(), Some(Error::InvalidState));
}

#[test]
fn weighted_final_setup_requires_creator_seed_and_keeps_surplus_separate() {
    for release in [
        CreatorAllocationRelease::AfterEnd,
        CreatorAllocationRelease::AfterSetup,
    ] {
        let (input, previous, pool) = prepared(release);
        let state = LbpLaunchState::decode(previous.post_state.as_ref().unwrap()).unwrap();
        let extra = |seed_balance, vault_balance| {
            vec![
                pool.clone(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, vault_balance),
                holding(id(42), state.collateral_definition_id, seed_balance),
            ]
        };
        assert_eq!(
            advance(&input, &previous, extra(99, 41)).err(),
            Some(Error::InvalidCustody)
        );
        assert_eq!(
            advance(&input, &previous, extra(100, u128::MAX)).err(),
            Some(Error::Overflow)
        );
        let response = advance(&input, &previous, extra(100, 41)).unwrap();
        if release == CreatorAllocationRelease::AfterEnd {
            assert!(response.events.is_empty());
        } else {
            assert!(matches!(
                &launch_events(&response)[..],
                [SaleEventV1 {
                    kind: SaleEventKindV1::AllocationReleased {
                        amount: 200,
                        release: AllocationReleaseV1::AfterSetup,
                        ..
                    },
                    ..
                }]
            ));
        }
        let next = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(next.creation_stage, LbpCreationStage::Complete);
        assert_eq!(
            next.creator_allocation_released,
            release == CreatorAllocationRelease::AfterSetup
        );
        assert!(response.sync_messages.iter().any(|call| call.to
            == Actor::new(id(42), factory_core::TOKEN_PROGRAM_ACCOUNT_ID)
            && borsh::from_slice::<lez_compat_core::token::Message>(&call.message).unwrap()
                == lez_compat_core::token::Message::Transfer {
                    definition: state.collateral_definition_id,
                    recipient: state.pool_id,
                    amount: 100,
                }));
        assert!(response.sync_messages.iter().any(|call| call.to
            == Actor::new(state.pool_id, state.lbp_program_id)
            && borsh::from_slice::<lbp_core::Message>(&call.message).unwrap()
                == lbp_core::Message::ActivateWeighted));
        let mut changed = lbp_core::PoolAccount::decode(&pool).unwrap();
        if let lbp_core::PoolMode::Weighted {
            collateral_seed, ..
        } = &mut changed.mode
        {
            *collateral_seed += 1;
        }
        let mut wrong = extra(100, 41);
        wrong[0] = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            advance(&input, &previous, wrong).err(),
            Some(Error::InvalidState)
        );
    }
}

#[test]
fn fixed_price_payout_commands_cannot_release_weighted_sale_funds() {
    let (mut input, response) = minted(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    for message in [
        Message::WithdrawFactoryProceeds {
            creator: id(42),
            now: 300,
        },
        Message::ClaimCreatorAllocation {
            creator: id(42),
            now: 300,
        },
    ] {
        input.message = borsh::to_vec(&message).unwrap();
        assert_eq!(handle(&input).err(), Some(Error::InvalidState));
    }
}

fn allowlisted_inputs() -> WeightedInputs {
    let mut terms = inputs();
    let factory = factory_core::compute_factory_pda(terms.namespace, id(7), terms.launch_salt);
    let pool = lbp_core::compute_pool_pda(
        terms.namespace,
        terms.lbp_program_id,
        factory_core::compute_definition_pda(terms.namespace, id(7), terms.launch_salt),
        terms.collateral_definition_id,
        factory,
    );
    terms.buy_policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *terms.namespace.value(),
        sale_policy_id: *pool.value(),
        root: [9; 32],
    });
    terms
}

#[test]
fn weighted_creation_saves_the_initial_bound_buy_policy() {
    let terms = allowlisted_inputs();
    let response = handle(&creation(terms.clone())).unwrap();
    assert_eq!(
        LbpLaunchState::decode(response.post_state.as_ref().unwrap())
            .unwrap()
            .buy_policy,
        terms.buy_policy
    );
}

#[test]
fn weighted_policy_is_immutable_through_setup_funding_and_retry() {
    let terms = allowlisted_inputs();
    let policy = terms.buy_policy.clone();
    let (input, prepared, bytes) = prepared_with_terms(terms);
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert_eq!(pool.buy_policy, policy);
    for case in 0..3 {
        let mut changed = pool.clone();
        match case {
            0 => changed.buy_policy = curve_core::BuyPolicy::Open,
            1 => {
                let curve_core::BuyPolicy::Allowlist(policy) = &mut changed.buy_policy else {
                    unreachable!()
                };
                policy.root = [99; 32];
            }
            _ => {}
        }
        let result = advance(
            &input,
            &prepared,
            vec![
                borsh::to_vec(&changed).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
                holding(id(42), state.collateral_definition_id, 100),
            ],
        );
        if case < 2 {
            assert_eq!(result.err(), Some(Error::InvalidState), "case {case}");
        } else {
            let response = result.unwrap();
            let complete = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
            assert_eq!(complete.buy_policy, policy);
            assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
        }
    }
}

#[test]
fn weighted_creation_rejects_wrong_policy_version_namespace_or_pool() {
    for case in 0..3 {
        let mut terms = allowlisted_inputs();
        let curve_core::BuyPolicy::Allowlist(policy) = &mut terms.buy_policy else {
            unreachable!()
        };
        match case {
            0 => policy.version = 2,
            1 => policy.namespace = [99; 32],
            _ => policy.sale_policy_id = [99; 32],
        }
        assert_eq!(
            handle(&creation(terms)).err(),
            Some(Error::InvalidTerms),
            "case {case}"
        );
    }
}

#[test]
fn weighted_pause_policy_survives_setup_and_rejects_substitution() {
    for policy in [
        curve_core::PausePolicy::Disabled,
        curve_core::PausePolicy::AuthorityControlled,
    ] {
        let mut terms = inputs();
        terms.pause_policy = policy;
        let (input, prepared, bytes) = prepared_with_terms(terms);
        let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.pause_policy, policy);
        let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
        assert_eq!(pool.trade_control.policy(), policy);
        for control in [
            curve_core::TradeControl::Unrestricted,
            curve_core::TradeControl::Running,
            curve_core::TradeControl::Paused,
        ] {
            let mut pool = pool.clone();
            pool.trade_control = control;
            let result = advance(
                &input,
                &prepared,
                vec![
                    borsh::to_vec(&pool).unwrap(),
                    holding(state.pool_id, state.token_definition_id, 0),
                    holding(state.pool_id, state.collateral_definition_id, 0),
                    holding(id(42), state.collateral_definition_id, 100),
                ],
            );
            if control.policy() != policy {
                assert_eq!(result.err(), Some(Error::InvalidState));
            } else {
                let complete =
                    LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(complete.pause_policy, policy);
                assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            }
        }
    }
}

#[test]
fn shared_pause_requires_the_sale_creator_on_every_call_before_pool_forwarding() {
    let (input, response) = minted(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    state.pause_policy = curve_core::PausePolicy::AuthorityControlled;
    assert!(matches!(
        state.mode,
        factory_core::LbpSetupMode::Weighted { .. }
    ));
    lbp_pause_forwarding::assert_creator_authority_and_pool_forwarding(input, state);
}

#[test]
fn legacy_weighted_pause_keeps_its_weighted_pool_alias() {
    let (mut input, response) = minted(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.message = borsh::to_vec(&Message::SetWeightedPaused {
        creator: id(42),
        paused: true,
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    input.pre_state = response.post_state.unwrap();
    input.from = Some(Actor::new(
        state.creator_role_id,
        factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
    ));
    input.message = borsh::to_vec(&lez_compat_core::authority::Authorized {
        role: state.creator_role_id,
        authority: id(42),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    assert_eq!(response.sync_messages.len(), 1);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(state.pool_id, state.lbp_program_id)
    );
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[0].message).unwrap(),
        lbp_core::Message::SetWeightedPaused { paused: true }
    );
}

#[test]
fn weighted_creation_requires_a_supported_ending_and_a_valid_schedule() {
    use factory_core::EndingPolicy;
    for ending in [
        EndingPolicy::Manual,
        EndingPolicy::Scheduled,
        EndingPolicy::Depletion,
        EndingPolicy::Either,
    ] {
        let mut terms = inputs();
        terms.ending_policy = ending;
        let result = handle(&creation(terms));
        if matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either) {
            assert!(result.is_ok());
        } else {
            assert_eq!(result.err(), Some(Error::InvalidTerms));
        }
    }
    for case in 0..2 {
        let mut terms = inputs();
        if case == 0 {
            terms.start_timestamp = u64::MAX;
        } else {
            terms.end_timestamp = u64::MAX;
        }
        assert_eq!(handle(&creation(terms)).err(), Some(Error::InvalidTerms));
    }
}

#[test]
fn weighted_policy_choices_survive_setup_and_pool_mismatches_reject_funding() {
    let (input, prepared, bytes) = prepared(CreatorAllocationRelease::AfterEnd);
    let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert_eq!(state.ending_policy, factory_core::EndingPolicy::Scheduled);
    assert_eq!(pool.ending_policy, state.ending_policy);
    for case in 0..3 {
        let mut changed = pool.clone();
        match case {
            0 => changed.ending_policy = factory_core::EndingPolicy::Manual,
            1 => changed.ending_policy = factory_core::EndingPolicy::Depletion,
            _ => {}
        }
        let result = advance(
            &input,
            &prepared,
            vec![
                borsh::to_vec(&changed).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
                holding(id(42), state.collateral_definition_id, 100),
            ],
        );
        if case < 2 {
            assert_eq!(result.err(), Some(Error::InvalidCustody));
        } else {
            let complete =
                LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
            assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            assert_eq!(complete.ending_policy, state.ending_policy);
        }
    }
}

fn weighted_authorized(input: &ReceiveInput, response: &Response) -> ReceiveInput {
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    ReceiveInput {
        receiver: input.receiver,
        from: Some(Actor::new(
            state.creator_role_id,
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&lez_compat_core::authority::Authorized {
            role: state.creator_role_id,
            authority: id(42),
        })
        .unwrap(),
    }
}

#[test]
fn scheduled_weighted_allocation_keeps_deadline_and_settlement_delegates_to_pool() {
    let (input, prepared, bytes) = prepared(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.remaining_inventory = 0;
    pool.recorded_collateral = 100;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut pool.mode
    {
        *pricing_collateral = 100;
    }
    for released in [false, true] {
        state.creator_allocation_released = released;
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message =
            borsh::to_vec(&Message::ClaimWeightedAllocation { creator: id(42) }).unwrap();
        let response = handle(&request).unwrap();
        request = weighted_authorized(&request, &response);
        let mut response = handle(&request).unwrap();
        {
            for bytes in [
                borsh::to_vec(&pool).unwrap(),
                holding(state.creator_escrow_id, state.token_definition_id, 200),
            ] {
                request = reply(&request, &response, bytes);
                response = handle(&request).unwrap();
            }
        }
        assert_eq!(response.validity.timestamps.start(), Some(300_000));
        assert!(!response.validity.timestamps.is_valid_for(299_999));
        assert!(response.validity.timestamps.is_valid_for(300_000));
        assert_eq!(response.sync_messages.len(), usize::from(!released));
        if released {
            assert!(response.events.is_empty());
        } else {
            assert!(matches!(
                &launch_events(&response)[..],
                [SaleEventV1 {
                    kind: SaleEventKindV1::AllocationReleased {
                        amount: 200,
                        release: AllocationReleaseV1::AfterEnd,
                        ..
                    },
                    ..
                }]
            ));
        }
    }
    let mut request = input;
    request.from = None;
    request.pre_state = borsh::to_vec(&state).unwrap().into();
    request.message =
        borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
    let response = handle(&request).unwrap();
    request = weighted_authorized(&request, &response);
    let response = handle(&request).unwrap();
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[0].message).unwrap(),
        lbp_core::Message::ReadState
    );
    request = reply(&request, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&request).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(300_000));
    assert!(!response.validity.timestamps.is_valid_for(299_999));
    assert!(response.validity.timestamps.is_valid_for(300_000));
    assert_eq!(response.sync_messages.len(), 2);
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&response.sync_messages[1].message).unwrap(),
        lbp_core::Message::Settle {
            route: factory_core::SettlementRoute::Manual,
            creator: id(42),
            adapter: None
        }
    );
}

#[test]
fn weighted_malformed_saved_policies_cannot_resume_or_forward_payouts() {
    let input = creation(inputs());
    let response = handle(&input).unwrap();
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    for case in 0..3 {
        let mut changed = state.clone();
        match case {
            0 => changed.ending_policy = factory_core::EndingPolicy::Manual,
            1 => changed.ending_policy = factory_core::EndingPolicy::Depletion,
            _ => changed.end_timestamp = None,
        }
        let bytes = borsh::to_vec(&changed).unwrap();
        assert_eq!(
            LbpLaunchState::decode(&bytes).err(),
            Some(Error::InvalidState)
        );
        let mut next = reply(&input, &response, config());
        next.pre_state = bytes.into();
        assert_eq!(handle(&next).err(), Some(Error::InvalidState));
        changed.creation_stage = LbpCreationStage::Complete;
        next.pre_state = borsh::to_vec(&changed).unwrap().into();
        next.message =
            borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
        assert_eq!(handle(&next).err(), Some(Error::InvalidState));
    }
}

fn weighted_callback<T: borsh::BorshSerialize>(
    input: &ReceiveInput,
    response: &Response,
    message: &T,
) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(response.sync_messages.last().unwrap().to),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(message).unwrap(),
    }
}

#[test]
fn weighted_hook_saves_adapter_binding_and_keeps_seed_outside_the_intent() {
    use factory_core::{
        AdapterMessage, GraduationIntent, GraduationIntentKind, GraduationResult,
        SettlementAmounts, SettlementProgress, SettlementReceipt, SettlementReply, SettlementRoute,
    };
    let mut terms = inputs();
    terms.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: b"weighted".to_vec(),
    });
    let (mut input, prepared, bytes) = prepared_with_terms(terms.clone());
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.graduation_hook, terms.graduation_hook);
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    let adapter = Actor::new(
        factory_core::compute_graduation_pda(id(9), input.receiver.account_id),
        id(9),
    );
    assert_eq!(pool.graduation_adapter, Some(adapter));
    let mut changed = pool.clone();
    changed.graduation_adapter = None;
    assert_eq!(
        advance(
            &input,
            &prepared,
            vec![
                borsh::to_vec(&changed).unwrap(),
                holding(state.pool_id, state.token_definition_id, 0),
                holding(state.pool_id, state.collateral_definition_id, 0),
                holding(id(42), state.collateral_definition_id, 100)
            ]
        )
        .err(),
        Some(Error::InvalidCustody)
    );
    state.creation_stage = LbpCreationStage::Complete;
    pool.funded = true;
    pool.remaining_inventory = 0;
    pool.recorded_collateral = 125;
    pool.accrued_fees = 3;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::ExecuteGraduationHook {
        creator_hint: Some(id(99)),
    })
    .unwrap();
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let response = handle(&input).unwrap();
    let adapter_state = factory_core::handle_adapter(&ReceiveInput {
        receiver: adapter,
        from: None,
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: state.namespace,
            launch_salt: state.launch_salt,
            accepted_intent: Some(GraduationIntentKind::WeightedBuyerProceeds),
        })
        .unwrap(),
    })
    .unwrap()
    .post_state
    .unwrap();
    input = reply(&input, &response, adapter_state.to_vec());
    let mut response = handle(&input).unwrap();
    assert_eq!(response.validity.timestamps.start(), Some(300_000));
    assert!(!response.validity.timestamps.is_valid_for(299_999));
    for definition in [state.token_definition_id, state.collateral_definition_id] {
        input = reply(
            &input,
            &response,
            holding(adapter.account_id, definition, 0),
        );
        response = handle(&input).unwrap();
    }
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(
            state.creator_role_id,
            factory_core::AUTHORITY_PROGRAM_ACCOUNT_ID
        )
    );
    input = weighted_callback(
        &input,
        &response,
        &lez_compat_core::authority::CurrentAuthority {
            role: state.creator_role_id,
            authority: id(99),
        },
    );
    let pending = handle(&input).unwrap();
    assert_eq!(pending.validity.timestamps.start(), Some(300_000));
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&pending.sync_messages.last().unwrap().message)
            .unwrap(),
        lbp_core::Message::Settle {
            route: SettlementRoute::Graduation,
            creator: id(99),
            adapter: Some(adapter)
        }
    );
    let receipt = SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: SettlementRoute::Graduation,
        creator: id(99),
        adapter: Some(adapter),
        amounts: SettlementAmounts {
            launch_tokens: 0,
            collateral: 25,
        },
        seed_payment: 100,
        progress: SettlementProgress::Graduated,
    };
    for case in 0..3 {
        let mut changed = receipt.clone();
        match case {
            0 => changed.seed_payment = 0,
            1 => changed.amounts.collateral = 125,
            _ => changed.amounts.launch_tokens = 1,
        }
        let bad = weighted_callback(
            &input,
            &pending,
            &SettlementReply::Applied { receipt: changed },
        );
        assert_eq!(handle(&bad).err(), Some(Error::InvalidState));
    }
    input = weighted_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let callback = handle(&input).unwrap();
    assert!(pending.events.is_empty());
    assert!(callback.events.is_empty());
    assert!(callback.sync_messages[0].pda_seeds.is_empty());
    let AdapterMessage::Graduate { request } =
        borsh::from_slice(&callback.sync_messages[0].message).unwrap()
    else {
        panic!("graduate");
    };
    assert_eq!(
        request.intent,
        GraduationIntent::WeightedBuyerProceeds { collateral: 25 }
    );
    assert_eq!(
        LbpLaunchState::decode(callback.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Unselected
    );
    input = weighted_callback(
        &input,
        &callback,
        &GraduationResult {
            factory: input.receiver.account_id,
            request_hash: request.hash(),
        },
    );
    let finalize = handle(&input).unwrap();
    assert!(finalize.events.is_empty());
    assert_eq!(
        borsh::from_slice::<lbp_core::Message>(&finalize.sync_messages[0].message).unwrap(),
        lbp_core::Message::FinalizeSettlement {
            receipt_hash: receipt.hash()
        }
    );
    input = weighted_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let completed = handle(&input).unwrap();
    assert_eq!(
        launch_events(&completed),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::Weighted,
            EventSubjectV1::Launch {
                factory: *input.receiver.account_id.value(),
                pool: *state.pool_id.value()
            },
            SaleEventKindV1::Graduated {
                adapter_program: *adapter.program_account_id.value(),
                adapter_custody: *adapter.account_id.value(),
                launch_tokens: 0,
                collateral: 25
            }
        )]
    );
    assert_eq!(
        LbpLaunchState::decode(completed.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Graduated
    );
}

#[test]
fn weighted_seed_only_manual_completion_is_final_and_acknowledged_once() {
    use factory_core::{
        SettlementAmounts, SettlementProgress, SettlementReceipt, SettlementReply, SettlementRoute,
    };
    let (mut input, prepared, bytes) = prepared(CreatorAllocationRelease::AfterEnd);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.recorded_collateral = 100;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
    let response = handle(&input).unwrap();
    input = weighted_authorized(&input, &response);
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let pending = handle(&input).unwrap();
    assert_eq!(pending.validity.timestamps.start(), Some(300_000));
    let receipt = SettlementReceipt {
        factory: input.receiver,
        pool: Actor::new(state.pool_id, state.lbp_program_id),
        route: SettlementRoute::Manual,
        creator: id(42),
        adapter: None,
        amounts: SettlementAmounts {
            launch_tokens: 0,
            collateral: 0,
        },
        seed_payment: 100,
        progress: SettlementProgress::ManualComplete,
    };
    input = weighted_callback(
        &input,
        &pending,
        &SettlementReply::Applied {
            receipt: receipt.clone(),
        },
    );
    let finalize = handle(&input).unwrap();
    assert!(finalize.events.is_empty());
    assert_eq!(
        LbpLaunchState::decode(finalize.post_state.as_ref().unwrap())
            .unwrap()
            .settlement_progress,
        SettlementProgress::Unselected
    );
    input = weighted_callback(
        &input,
        &finalize,
        &SettlementReply::Completed {
            receipt_hash: receipt.hash(),
            progress: receipt.progress,
        },
    );
    let completed = handle(&input).unwrap();
    assert!(completed.events.is_empty());
    let state = LbpLaunchState::decode(completed.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        state.settlement_progress,
        SettlementProgress::ManualComplete
    );
    pool.settlement_progress = state.settlement_progress;
    pool.seed_returned = true;
    pool.recorded_collateral = 0;
    input.pre_state = completed.post_state.unwrap();
    input.from = None;
    input.message = borsh::to_vec(&Message::WithdrawWeightedProceeds { creator: id(42) }).unwrap();
    let response = handle(&input).unwrap();
    input = weighted_authorized(&input, &response);
    let response = handle(&input).unwrap();
    input = reply(&input, &response, borsh::to_vec(&pool).unwrap());
    let noop = handle(&input).unwrap();
    assert!(noop.events.is_empty());
    assert!(noop.sync_messages.is_empty());
    assert_eq!(noop.validity.timestamps.start(), Some(300_000));
}

#[test]
fn weighted_allocation_and_retries_use_selected_close_without_releasing_schedule_dust() {
    for ending in [
        factory_core::EndingPolicy::Scheduled,
        factory_core::EndingPolicy::Either,
    ] {
        let mut terms = inputs();
        terms.ending_policy = ending;
        let (input, prepared, bytes) = prepared_with_terms(terms);
        let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        state.creation_stage = LbpCreationStage::Complete;
        let original = lbp_core::PoolAccount::decode(&bytes).unwrap();
        for remaining in [0, 1] {
            let mut pool = original.clone();
            pool.funded = true;
            pool.remaining_inventory = remaining;
            pool.recorded_collateral = 100;
            if let lbp_core::PoolMode::Weighted {
                pricing_collateral, ..
            } = &mut pool.mode
            {
                *pricing_collateral = 100;
            }
            for released in [false, true] {
                state.creator_allocation_released = released;
                let mut request = input.clone();
                request.from = None;
                request.pre_state = borsh::to_vec(&state).unwrap().into();
                request.message =
                    borsh::to_vec(&Message::ClaimWeightedAllocation { creator: id(42) }).unwrap();
                let response = handle(&request).unwrap();
                request = weighted_authorized(&request, &response);
                let mut response = handle(&request).unwrap();
                for bytes in [
                    borsh::to_vec(&pool).unwrap(),
                    holding(
                        state.creator_escrow_id,
                        state.token_definition_id,
                        if released { 0 } else { 200 },
                    ),
                ] {
                    request = reply(&request, &response, bytes);
                    response = handle(&request).unwrap();
                }
                let expected = if ending == factory_core::EndingPolicy::Either && remaining == 0 {
                    None
                } else {
                    Some(300_000)
                };
                assert_eq!(response.validity.timestamps.start(), expected);
                assert_eq!(response.sync_messages.len(), usize::from(!released));
                assert!(
                    LbpLaunchState::decode(response.post_state.as_ref().unwrap())
                        .unwrap()
                        .creator_allocation_released
                );
            }
        }
    }
}

#[test]
fn released_after_setup_allocation_retry_stays_available_while_weighted_sale_is_open() {
    let (input, prepared, bytes) = prepared(CreatorAllocationRelease::AfterSetup);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    state.creator_allocation_released = true;
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    assert!(pool.remaining_inventory > 0);
    assert_eq!(pool.close_time_bound().unwrap(), Some(300));
    let mut request = input.clone();
    request.from = None;
    request.pre_state = borsh::to_vec(&state).unwrap().into();
    request.message = borsh::to_vec(&Message::ClaimWeightedAllocation { creator: id(42) }).unwrap();
    let response = handle(&request).unwrap();
    let mut callback = weighted_authorized(&request, &response);
    callback.from = None;
    assert_eq!(handle(&callback).err(), Some(Error::Unauthorized));
    let repeated = handle(&weighted_authorized(&request, &response)).unwrap();
    assert!(repeated.sync_messages.is_empty());
    assert_eq!(repeated.validity.timestamps.start(), None);
    assert_eq!(
        LbpLaunchState::decode(repeated.post_state.as_ref().unwrap()).unwrap(),
        state
    );
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
fn weighted_created_event_names_the_launch_and_original_seed_once() {
    let input = creation(inputs());
    let response = handle(&input).unwrap();
    let state = LbpLaunchState::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(
        launch_events(&response),
        vec![SaleEventV1::new(
            *state.namespace.value(),
            PricingEngineV1::Weighted,
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
                creator_allocation: 200,
                curve_reserve: 0,
                collateral_seed: 100,
                start_timestamp: Some(150),
                end_timestamp: Some(300),
                policies: EventPoliciesV1 {
                    ending: state.ending_policy,
                    pause: state.pause_policy
                }
            }
        )]
    );
    let (_, minted) = minted(CreatorAllocationRelease::AfterEnd);
    assert!(minted.events.is_empty());
    let mut replay = input;
    replay.pre_state = minted.post_state.unwrap();
    assert_eq!(handle(&replay).err(), Some(Error::AlreadyInitialized));
}

#[test]
fn weighted_trade_mode_survives_setup_and_rejects_substitution() {
    for mode in [
        factory_core::TradeMode::BuyOnly,
        factory_core::TradeMode::BuyAndSell,
    ] {
        let mut terms = inputs();
        terms.trade_mode = mode;
        let (input, prepared, bytes) = prepared_with_terms(terms);
        let state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
        assert_eq!(state.trade_mode, mode);
        let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
        assert_eq!(pool.trade_mode, mode);
        for pool_mode in [
            factory_core::TradeMode::BuyOnly,
            factory_core::TradeMode::BuyAndSell,
        ] {
            let mut pool = pool.clone();
            pool.trade_mode = pool_mode;
            let result = advance(
                &input,
                &prepared,
                vec![
                    borsh::to_vec(&pool).unwrap(),
                    holding(state.pool_id, state.token_definition_id, 0),
                    holding(state.pool_id, state.collateral_definition_id, 0),
                    holding(id(42), state.collateral_definition_id, 100),
                ],
            );
            if pool_mode != mode {
                assert_eq!(result.err(), Some(Error::InvalidCustody));
            } else {
                let complete =
                    LbpLaunchState::decode(result.unwrap().post_state.as_ref().unwrap()).unwrap();
                assert_eq!(complete.trade_mode, mode);
                assert_eq!(complete.creation_stage, LbpCreationStage::Complete);
            }
        }
    }
}

#[test]
fn weighted_payouts_and_hook_reject_a_substituted_trade_mode() {
    let mut terms = inputs();
    terms.graduation_hook = Some(factory_core::GraduationHook {
        program_id: id(9),
        configuration: vec![],
    });
    let (input, prepared, bytes) = prepared_with_terms(terms);
    let mut state = LbpLaunchState::decode(prepared.post_state.as_ref().unwrap()).unwrap();
    state.creation_stage = LbpCreationStage::Complete;
    let mut pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    pool.funded = true;
    pool.trade_mode = factory_core::TradeMode::BuyAndSell;
    for command in [
        Message::ClaimWeightedAllocation { creator: id(42) },
        Message::WithdrawWeightedProceeds { creator: id(42) },
        Message::ExecuteGraduationHook {
            creator_hint: Some(id(42)),
        },
    ] {
        let hook = matches!(command, Message::ExecuteGraduationHook { .. });
        let allocation = matches!(command, Message::ClaimWeightedAllocation { .. });
        let mut request = input.clone();
        request.from = None;
        request.pre_state = borsh::to_vec(&state).unwrap().into();
        request.message = borsh::to_vec(&command).unwrap();
        let mut response = handle(&request).unwrap();
        if !hook {
            request = weighted_authorized(&request, &response);
            response = handle(&request).unwrap();
        }
        request = reply(&request, &response, borsh::to_vec(&pool).unwrap());
        if allocation {
            response = handle(&request).unwrap();
            request = reply(
                &request,
                &response,
                holding(state.creator_escrow_id, state.token_definition_id, 200),
            );
        }
        assert_eq!(handle(&request).err(), Some(Error::InvalidCustody));
    }
}
