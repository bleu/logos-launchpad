use lbp_core::{CreatorAllocationRelease, Message, WeightedPoolInputs, compute_pool_pda, handle};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput},
};

fn weight(percent: u128) -> u128 {
    percent * lbp_core::WEIGHT_TOTAL / 100
}

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}

fn weighted_inputs() -> WeightedPoolInputs {
    WeightedPoolInputs {
        namespace: id(40),
        namespace_program_id: id(6),
        factory_program_id: id(7),
        factory_seed: [3; 32],
        token_definition_id: id(41),
        collateral_definition_id: id(42),
        sale_inventory: 1_000_000,
        collateral_seed: 100,
        creator_allocation: 0,
        token_weight_start: weight(95),
        token_weight_end: weight(50),
        start_timestamp: 100,
        end_timestamp: 200,
        creator_allocation_release: CreatorAllocationRelease::AfterSetup,
    }
}

fn prepare() -> ReceiveInput {
    let inputs = weighted_inputs();
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(factory, id(7))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap(),
    }
}

fn namespace_state() -> Vec<u8> {
    let input = ReceiveInput {
        receiver: Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6)),
        from: Some(Actor::new(id(40), id(6))),
        is_authorized: false,
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

fn reply(
    input: &ReceiveInput,
    response: &lee_core::program::Response,
    from: Actor,
    bytes: Vec<u8>,
) -> ReceiveInput {
    ReceiveInput {
        receiver: input.receiver,
        from: Some(from),
        is_authorized: false,
        pre_state: response.post_state.clone().unwrap(),
        message: borsh::to_vec(&lee_core::program::StateReply {
            state: bytes.into(),
        })
        .unwrap(),
    }
}

fn definition() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Token".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 2_000_000,
        metadata: None,
    })
    .unwrap()
}

fn holding(owner: AccountId, definition: AccountId, balance: u128) -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Holding {
        owner,
        definition,
        kind: lez_compat_core::token::TokenKind::Fungible,
        balance,
    })
    .unwrap()
}

/// Drive the prepare → namespace → definitions → custody handshake through
/// to completion. Returns (final_input, final_response).
fn prepared() -> (ReceiveInput, lee_core::program::Response) {
    let inputs = weighted_inputs();
    let factory = prepare().from.unwrap().account_id;
    let mut input = prepare();
    let mut response = handle_completed(&input);
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(factory, id(41), inputs.sale_inventory),
        holding(input.receiver.account_id, id(41), 0),
        holding(input.receiver.account_id, id(42), 0),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle_completed(&input);
    }
    (input, response)
}

// ---------------------------------------------------------------------------
// PrepareWeighted — term validation
// ---------------------------------------------------------------------------

#[test]
fn weighted_preparation_rejects_invalid_terms() {
    for case in 0..10u8 {
        let mut input = prepare();
        let Message::PrepareWeighted { mut inputs } = borsh::from_slice(&input.message).unwrap()
        else {
            unreachable!()
        };
        let expected = match case {
            0 => {
                input.pre_state = vec![1].into();
                lbp_core::Error::AlreadyInitialized
            }
            1 => {
                input.receiver.account_id = id(99);
                lbp_core::Error::WrongAccount
            }
            2 => {
                input.from = None;
                lbp_core::Error::Unauthorized
            }
            3 => {
                inputs.sale_inventory = 0;
                lbp_core::Error::InvalidTerms
            }
            4 => {
                inputs.token_weight_start = 0;
                lbp_core::Error::InvalidTerms
            }
            5 => {
                inputs.token_weight_end = 0;
                lbp_core::Error::InvalidTerms
            }
            6 => {
                // A token weight equal to the total gives a zero collateral weight.
                inputs.token_weight_start = lbp_core::WEIGHT_TOTAL;
                lbp_core::Error::InvalidTerms
            }
            7 => {
                // end_timestamp <= start_timestamp
                inputs.end_timestamp = 100;
                lbp_core::Error::InvalidTerms
            }
            8 => {
                inputs.token_definition_id = inputs.collateral_definition_id;
                lbp_core::Error::InvalidTerms
            }
            _ => {
                inputs.collateral_seed = 0;
                lbp_core::Error::InvalidTerms
            }
        };
        if case != 2 {
            input.message = borsh::to_vec(&Message::PrepareWeighted { inputs }).unwrap();
        }
        assert!(
            matches!(handle(&input), Err(e) if e == expected),
            "case {case}: expected {expected:?}"
        );
    }
}

#[test]
fn weighted_preparation_checks_namespace_through_its_actor() {
    let input = prepare();
    let response = handle_completed(&input);
    assert_eq!(
        response.sync_messages[0].to,
        Actor::new(curve_core::compute_config_pda(id(40), id(6)), id(6))
    );
    assert_eq!(
        borsh::from_slice::<curve_core::Message>(&response.sync_messages[0].message).unwrap(),
        curve_core::Message::ReadNamespace { namespace: id(40) }
    );
}

#[test]
fn weighted_preparation_saves_pending_terms_and_initializes_vaults() {
    let (input, response) = prepared();
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(!state.funded);
    assert_eq!(state.recorded_collateral, 0);
    assert_eq!(state.accrued_fees, 0);
    assert_eq!(
        (state.start_timestamp, state.end_timestamp),
        (100, Some(200))
    );
    assert_eq!(state.sale_inventory, 1_000_000);
    assert_eq!(state.remaining_inventory, 1_000_000);
    assert_eq!(state.factory_owner, prepare().from.unwrap().account_id);
    assert_eq!(
        input.receiver.account_id,
        compute_pool_pda(id(40), id(8), id(41), id(42), state.factory_owner)
    );
    assert!(response.async_messages.is_empty());
    // mode must be Weighted
    assert!(matches!(state.mode, lbp_core::PoolMode::Weighted { .. }));
}

// ---------------------------------------------------------------------------
// ActivateWeighted
// ---------------------------------------------------------------------------

fn activation(
    token_vault: u128,
    collateral_vault: u128,
) -> (ReceiveInput, lee_core::program::Response) {
    let (mut input, prepared) = prepared();
    input.pre_state = prepared.post_state.unwrap();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    let mut response = handle_completed(&input);
    let inputs = weighted_inputs();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(input.from.unwrap().account_id, id(41), 0),
        holding(input.receiver.account_id, id(41), token_vault),
        holding(input.receiver.account_id, id(42), collateral_vault),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle_completed(&input);
    }
    let _ = inputs;
    (input, response)
}

#[test]
fn weighted_activation_funds_and_records_seed_collateral() {
    let inputs = weighted_inputs();
    let (mut input, response) = activation(inputs.sale_inventory, inputs.collateral_seed);
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(state.funded);
    assert_eq!(state.remaining_inventory, inputs.sale_inventory);
    // recorded_collateral starts at the seed so the vault liabilities balance
    assert_eq!(state.recorded_collateral, inputs.collateral_seed);
    assert_eq!(state.accrued_fees, 0);
    assert!(matches!(state.mode, lbp_core::PoolMode::Weighted {
        collateral_seed,
        pricing_collateral,
        ..
    } if collateral_seed == inputs.collateral_seed && pricing_collateral == inputs.collateral_seed));
    // double-funding must fail
    input.pre_state = response.post_state.unwrap();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
    assert!(matches!(
        handle(&input),
        Err(lbp_core::Error::AlreadyFunded)
    ));
}

#[test]
fn weighted_activation_excludes_collateral_surplus_from_pricing() {
    let inputs = weighted_inputs();
    let (_, response) = activation(inputs.sale_inventory, inputs.collateral_seed + 999);
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert_eq!(state.recorded_collateral, inputs.collateral_seed);
    assert!(matches!(state.mode, lbp_core::PoolMode::Weighted {
        pricing_collateral,
        ..
    } if pricing_collateral == inputs.collateral_seed));
}

#[test]
fn weighted_activation_rejects_wrong_token_vault_balance() {
    let inputs = weighted_inputs();
    for wrong in [0u128, inputs.sale_inventory - 1, inputs.sale_inventory + 1] {
        let (mut input, prepared) = prepared();
        input.pre_state = prepared.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateWeighted).unwrap();
        let mut response = handle_completed(&input);
        for (index, bytes) in [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), 0),
            holding(input.receiver.account_id, id(41), wrong),
        ]
        .into_iter()
        .enumerate()
        {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            let result = handle(&input);
            if index == 4 {
                assert!(
                    matches!(result, Err(lbp_core::Error::InvalidCustody)),
                    "wrong={wrong}"
                );
                break;
            }
            response = result.unwrap();
        }
    }
}

fn handle_completed(input: &ReceiveInput) -> lee_core::program::Response {
    let mut response = handle(input).unwrap();
    while response
        .post_state
        .as_ref()
        .is_some_and(|state| state.starts_with(b"LZCLK001"))
    {
        assert_eq!(response.sync_messages[0].to, clock_core::clock_actor());
        response = handle(&ReceiveInput {
            receiver: input.receiver,
            from: Some(clock_core::clock_actor()),
            is_authorized: false,
            pre_state: response.post_state.unwrap(),
            message: borsh::to_vec(&clock_core::Reply::Timestamp(100_000)).unwrap(),
        })
        .unwrap();
    }
    response
}
