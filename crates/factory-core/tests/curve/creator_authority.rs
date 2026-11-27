use super::*;

#[test]
fn curve_creation_bootstraps_a_transfer_only_creator_role_with_the_mint() {
    let (_, response, _) = minted();
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
    let (_, response, _) = minted();
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
    let (_, response, setup) = minted();
    let mut input = setup.continuation(&response, LbpCreationStage::Minted);
    let mut state = FactoryState::decode(&input.pre_state).unwrap();
    state.creator_role_id = compute_curve_creator_role_pda(id(41), id(7), [3; 32]);
    input.pre_state = borsh::to_vec(&state).unwrap().into();
    assert!(matches!(handle(&input), Err(Error::WrongAccount)));
}

#[test]
fn continuation_checks_the_creator_before_any_custody_call() {
    let (_, response, setup) = minted();
    setup.assert_rejects_external_continuation(&response);
    let input = setup.continuation(&response, LbpCreationStage::Minted);
    let state = FactoryState::decode(&input.pre_state).unwrap();
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
    let (input, minted, _) = minted();
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

    for (settlement_stage, command) in [
        (
            SettlementStage::Unstarted,
            Message::SetPaused {
                creator: id(99),
                paused: true,
            },
        ),
        (
            SettlementStage::Unstarted,
            Message::ClaimCreatorAllocation {
                creator: id(99),
                now: 400,
            },
        ),
        (
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
        state.creation_stage = CreationStage::Active;
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
    let (input, minted, setup) = minted();
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
        let mut state = FactoryState::decode(minted.post_state.as_ref().unwrap()).unwrap();
        let request = if matches!(command, Message::ContinueCreation { .. }) {
            setup.continuation(&minted, LbpCreationStage::Minted)
        } else {
            let mut request = input.clone();
            state.creation_stage = CreationStage::Active;
            request.pre_state = borsh::to_vec(&state).unwrap().into();
            request.message = borsh::to_vec(&command).unwrap();
            request
        };
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
