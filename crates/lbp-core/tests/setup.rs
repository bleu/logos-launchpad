use lbp_core::{
    CreatorAllocationRelease, FixedPrice, FixedPricePoolInputs, Message, compute_pool_pda, handle,
};
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput},
};

fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
fn prepare() -> ReceiveInput {
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let inputs = FixedPricePoolInputs {
        namespace: id(40),
        namespace_program_id: id(6),
        factory_program_id: id(7),
        factory_seed: [3; 32],
        token_definition_id: id(41),
        collateral_definition_id: id(42),
        sale_inventory: 800,
        creator_allocation: 200,
        price: FixedPrice {
            collateral_units: 3,
            token_units: 2,
        },
        start_timestamp: 100,
        end_timestamp: 200,
        creator_allocation_release: CreatorAllocationRelease::AfterSetup,
    };
    ReceiveInput {
        receiver: Actor::new(
            compute_pool_pda(id(40), id(8), id(41), id(42), factory),
            id(8),
        ),
        from: Some(Actor::new(factory, id(7))),
        is_authorized: false,
        pre_state: vec![].into(),
        message: borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap(),
    }
}

#[test]
fn preparation_checks_the_saved_namespace_through_its_actor() {
    let input = prepare();
    let response = handle(&input).unwrap();
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
fn setup_accepts_only_the_saved_factory_program_and_pda() {
    for from in [
        None,
        Some(Actor::new(id(99), id(7))),
        Some(Actor::new(prepare().from.unwrap().account_id, id(99))),
    ] {
        let mut input = prepare();
        input.from = from;
        assert!(matches!(handle(&input), Err(lbp_core::Error::Unauthorized)));
    }
}

#[test]
fn preparation_requires_fresh_canonical_pool_and_positive_distinct_terms() {
    for case in 0..8 {
        let mut input = prepare();
        let Message::PrepareFixedPrice { mut inputs } = borsh::from_slice(&input.message).unwrap()
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
                inputs.sale_inventory = 0;
                lbp_core::Error::InvalidTerms
            }
            3 => {
                inputs.price.collateral_units = 0;
                lbp_core::Error::InvalidTerms
            }
            4 => {
                inputs.price.token_units = 0;
                lbp_core::Error::InvalidTerms
            }
            5 => {
                inputs.end_timestamp = 100;
                lbp_core::Error::InvalidTerms
            }
            6 => {
                inputs.collateral_definition_id = inputs.token_definition_id;
                lbp_core::Error::InvalidTerms
            }
            _ => {
                inputs.namespace = AccountId::default();
                lbp_core::Error::InvalidTerms
            }
        };
        input.message = borsh::to_vec(&Message::PrepareFixedPrice { inputs }).unwrap();
        assert!(
            matches!(handle(&input),Err(e) if e==expected),
            "case {case}"
        );
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
#[test]
fn namespace_reply_must_come_from_the_saved_program_and_config_address() {
    let input = prepare();
    let response = handle(&input).unwrap();
    let source = response.sync_messages[0].to;
    for from in [
        Actor::new(id(99), source.program_account_id),
        Actor::new(source.account_id, id(99)),
    ] {
        assert!(matches!(
            handle(&reply(&input, &response, from, namespace_state())),
            Err(lbp_core::Error::Unauthorized)
        ));
    }
    let accepted = handle(&reply(&input, &response, source, namespace_state())).unwrap();
    assert_eq!(
        accepted.sync_messages[0].to,
        Actor::new(id(41), curve_core::TOKEN_PROGRAM_ACCOUNT_ID)
    );
}

fn definition() -> Vec<u8> {
    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
        name: "Token".into(),
        kind: lez_compat_core::token::TokenKind::Fungible,
        supply: 1000,
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
fn prepared() -> (ReceiveInput, lee_core::program::Response) {
    let mut input = prepare();
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(input.from.unwrap().account_id, id(41), 800),
        holding(input.receiver.account_id, id(41), 0),
        holding(input.receiver.account_id, id(42), 0),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}
#[test]
fn preparation_saves_pending_terms_and_initializes_canonical_vaults() {
    let (input, response) = prepared();
    let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    assert!(!state.funded);
    assert_eq!(state.recorded_collateral, 0);
    assert_eq!((state.start_timestamp, state.end_timestamp), (100, 200));
    assert_eq!((state.sale_inventory, state.creator_allocation), (800, 200));
    assert_eq!(
        state.price,
        FixedPrice {
            collateral_units: 3,
            token_units: 2
        }
    );
    assert_eq!(state.factory_owner, prepare().from.unwrap().account_id);
    assert_eq!(
        input.receiver.account_id,
        compute_pool_pda(id(40), id(8), id(41), id(42), state.factory_owner)
    );
    assert!(response.async_messages.is_empty());
}

#[test]
fn preparation_rejects_nonzero_inventory_or_collateral_in_pool_vaults() {
    for slot in [4, 5] {
        for amount in [1, 800, u128::MAX] {
            let mut input = prepare();
            let mut response = handle(&input).unwrap();
            let values = [
                namespace_state(),
                definition(),
                definition(),
                holding(input.from.unwrap().account_id, id(41), 800),
                holding(
                    input.receiver.account_id,
                    id(41),
                    if slot == 4 { amount } else { 0 },
                ),
                holding(
                    input.receiver.account_id,
                    id(42),
                    if slot == 5 { amount } else { 0 },
                ),
            ];
            for (index, bytes) in values.into_iter().enumerate() {
                let source = response.sync_messages.last().unwrap().to;
                input = reply(&input, &response, source, bytes);
                let result = handle(&input);
                if index == slot {
                    assert!(matches!(result, Err(lbp_core::Error::InvalidCustody)));
                    break;
                }
                response = result.unwrap();
            }
        }
    }
}

fn activation(collateral: u128) -> (ReceiveInput, lee_core::program::Response) {
    let (mut input, prepared) = prepared();
    input.pre_state = prepared.post_state.unwrap();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
    let mut response = handle(&input).unwrap();
    for bytes in [
        namespace_state(),
        definition(),
        definition(),
        holding(input.from.unwrap().account_id, id(41), 0),
        holding(input.receiver.account_id, id(41), 800),
        holding(input.receiver.account_id, id(42), collateral),
    ] {
        let source = response.sync_messages.last().unwrap().to;
        input = reply(&input, &response, source, bytes);
        response = handle(&input).unwrap();
    }
    (input, response)
}
#[test]
fn activation_funds_exact_inventory_and_keeps_donations_outside_recorded_collateral() {
    for donation in [0, 1, 800, u128::MAX] {
        let (mut input, response) = activation(donation);
        let state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
        assert!(state.funded);
        assert_eq!(state.recorded_collateral, 0);
        assert_eq!(state.sale_inventory, 800);
        input.pre_state = response.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
        assert!(matches!(
            handle(&input),
            Err(lbp_core::Error::AlreadyFunded)
        ));
    }
}

#[test]
fn activation_requires_zero_recorded_collateral_and_exact_credited_inventory() {
    let (mut input, response) = prepared();
    let mut state = lbp_core::PoolAccount::decode(response.post_state.as_ref().unwrap()).unwrap();
    state.recorded_collateral = 1;
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    input.from = prepare().from;
    input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
    assert!(matches!(
        handle(&input),
        Err(lbp_core::Error::RecordedCollateral)
    ));
    for (source, vault) in [(1, 800), (800, 0), (0, 799), (0, 801), (0, u128::MAX)] {
        let (mut input, response) = prepared();
        input.pre_state = response.post_state.unwrap();
        input.from = prepare().from;
        input.message = borsh::to_vec(&Message::ActivateFixedPrice).unwrap();
        let mut response = handle(&input).unwrap();
        for (index, bytes) in [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), source),
            holding(input.receiver.account_id, id(41), vault),
        ]
        .into_iter()
        .enumerate()
        {
            let from = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, from, bytes);
            let result = handle(&input);
            if index == if source != 0 { 3 } else { 4 } {
                assert!(matches!(result, Err(lbp_core::Error::InvalidCustody)));
                break;
            }
            response = result.unwrap();
        }
    }
}

#[test]
fn setup_rejects_substituted_token_actor_sources_and_invalid_canonical_holdings() {
    for slot in 1..6 {
        let mut input = prepare();
        let mut response = handle(&input).unwrap();
        let values = [
            namespace_state(),
            definition(),
            definition(),
            holding(input.from.unwrap().account_id, id(41), 800),
            holding(input.receiver.account_id, id(41), 0),
            holding(input.receiver.account_id, id(42), 0),
        ];
        for (index, bytes) in values.into_iter().enumerate() {
            let source = response.sync_messages.last().unwrap().to;
            input = reply(&input, &response, source, bytes);
            if index == slot {
                for from in [
                    Actor::new(id(99), source.program_account_id),
                    Actor::new(source.account_id, id(99)),
                ] {
                    let mut wrong = input.clone();
                    wrong.from = Some(from);
                    assert!(matches!(handle(&wrong), Err(lbp_core::Error::Unauthorized)));
                }
                let invalid = if slot < 3 {
                    borsh::to_vec(&lez_compat_core::token::TokenState::Definition {
                        name: "NFT".into(),
                        kind: lez_compat_core::token::TokenKind::NftMaster,
                        supply: 1,
                        metadata: None,
                    })
                    .unwrap()
                } else {
                    holding(id(99), id(99), 0)
                };
                let wrong = reply(&input, &response, source, invalid);
                assert!(handle(&wrong).is_err());
            }
            response = handle(&input).unwrap();
        }
    }
}

#[test]
fn setup_uses_the_trusted_clock_with_a_strict_saved_end() {
    for (_, response) in [prepared(), activation(17)] {
        let clock = response
            .sync_messages
            .iter()
            .find(|m| m.to.account_id == clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID)
            .expect("trusted clock assertion");
        assert_eq!(clock.to.program_account_id, clock_core::clock_account_id());
        assert_eq!(
            borsh::from_slice::<clock_core::Message>(&clock.message).unwrap(),
            clock_core::Message::AssertTimestamp {
                at_least: 0,
                at_most: 199_999
            }
        );
    }
}
