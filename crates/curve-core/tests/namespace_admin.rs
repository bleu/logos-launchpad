use curve_core::{Config, INITIAL_EMERGENCY_AUTHORITY, Message, compute_config_pda, handle};
use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::ReceiveInput,
};

const PROGRAM: AccountId = AccountId::new([42; 32]);
const NAMESPACE: AccountId = AccountId::new([1; 32]);
const TREASURY: AccountId = AccountId::new([6; 32]);

fn input(
    receiver: AccountId,
    state: ActorState,
    authorized: bool,
    message: Message,
) -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(receiver, PROGRAM),
        from: None,
        is_authorized: authorized,
        pre_state: state,
        message: borsh::to_vec(&message).unwrap(),
    }
}

#[test]
fn namespace_signer_creates_its_config_without_an_authority_nft() {
    let request = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::InitializeNamespace {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
            treasury: TREASURY,
        },
    );
    let forwarded = handle(&request).unwrap().sync_messages.remove(0);
    assert_eq!(
        forwarded.to,
        Actor::new(compute_config_pda(NAMESPACE, PROGRAM), PROGRAM)
    );
    let response = handle(&ReceiveInput {
        receiver: forwarded.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: forwarded.message,
    })
    .unwrap();
    let state = Config::decode(&response.post_state.unwrap()).unwrap();
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.treasury, TREASURY);
    assert_eq!(state.emergency.authority(), INITIAL_EMERGENCY_AUTHORITY);
    assert!(!state.emergency.is_frozen);
    assert_eq!(
        response.sync_messages.len(),
        1,
        "administrator bootstrap is synchronous"
    );
    if let Ok(expected) = std::env::var("EXPECTED_LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT") {
        assert_eq!(
            response.sync_messages[0].to.program_account_id,
            expected.parse::<AccountId>().unwrap()
        );
    }
}

#[test]
fn administrator_changes_only_the_fee_after_an_authenticated_role_reply() {
    let init = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::InitializeNamespace {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
            treasury: TREASURY,
        },
    );
    let forward = handle(&init).unwrap().sync_messages.remove(0);
    let config_actor = forward.to;
    let created = handle(&ReceiveInput {
        receiver: config_actor,
        from: Some(init.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: forward.message,
    })
    .unwrap();
    let before = created.post_state.unwrap();
    let request = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::UpdateFee {
            namespace: NAMESPACE,
            protocol_fee_bps: 123,
        },
    );
    let forwarded = handle(&request).unwrap().sync_messages.remove(0);
    let pending = handle(&ReceiveInput {
        receiver: config_actor,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: before,
        message: forwarded.message,
    })
    .unwrap();
    let proof = lez_compat_core::authority::Authorized {
        role: curve_core::compute_admin_role(NAMESPACE, PROGRAM),
        authority: NAMESPACE,
    };
    let mut reply = ReceiveInput {
        receiver: config_actor,
        from: Some(Actor::new(
            proof.role,
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: borsh::to_vec(&proof).unwrap(),
    };
    let updated = handle(&reply).unwrap();
    let state = Config::decode(&updated.post_state.unwrap()).unwrap();
    assert_eq!(state.protocol_fee_bps, 123);
    assert_eq!(state.treasury, TREASURY);
    reply.from = Some(Actor::new(proof.role, PROGRAM));
    assert!(handle(&reply).is_err());
}

#[test]
fn administrator_changes_only_the_treasury_after_an_authenticated_role_reply() {
    let init = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::InitializeNamespace {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
            treasury: TREASURY,
        },
    );
    let forward = handle(&init).unwrap().sync_messages.remove(0);
    let config_actor = forward.to;
    let created = handle(&ReceiveInput {
        receiver: config_actor,
        from: Some(init.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: forward.message,
    })
    .unwrap();
    let before = created.post_state.unwrap();
    let request = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::UpdateTreasury {
            namespace: NAMESPACE,
            treasury: AccountId::new([7; 32]),
        },
    );
    let forwarded = handle(&request).unwrap().sync_messages.remove(0);
    let pending = handle(&ReceiveInput {
        receiver: config_actor,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: before,
        message: forwarded.message,
    })
    .unwrap();
    let proof = lez_compat_core::authority::Authorized {
        role: curve_core::compute_admin_role(NAMESPACE, PROGRAM),
        authority: NAMESPACE,
    };
    let mut reply = ReceiveInput {
        receiver: config_actor,
        from: Some(Actor::new(
            proof.role,
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
        )),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: borsh::to_vec(&proof).unwrap(),
    };
    let updated = handle(&reply).unwrap();
    let state = Config::decode(&updated.post_state.unwrap()).unwrap();
    assert_eq!(state.protocol_fee_bps, 500);
    assert_eq!(state.treasury, AccountId::new([7; 32]));
    reply.from = Some(Actor::new(proof.role, PROGRAM));
    assert!(handle(&reply).is_err());
}

fn authority_call(
    call: lee_core::program::SyncMessage,
    from: Actor,
    state: ActorState,
    authorized: bool,
) -> Result<lee_core::program::Response, lez_compat_core::Error> {
    lez_compat_core::authority::handle(&ReceiveInput {
        receiver: call.to,
        from: Some(from),
        is_authorized: authorized,
        pre_state: state,
        message: call.message,
    })
}

#[test]
fn pda_successor_requires_owner_approval_and_can_then_administer_the_namespace() {
    use curve_core::{
        AUTHORITY_PROGRAM_ACCOUNT_ID as AUTHORITY, AdminCandidate, compute_admin_role,
    };
    use lee_core::program::PdaSeed;
    let owner_program = AccountId::new([81; 32]);
    let seed = PdaSeed::new([82; 32]);
    let successor = AccountId::for_public_pda(&owner_program, &seed);
    let config_actor = Actor::new(compute_config_pda(NAMESPACE, PROGRAM), PROGRAM);
    let role_actor = Actor::new(compute_admin_role(NAMESPACE, PROGRAM), AUTHORITY);
    let init = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::InitializeNamespace {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
            treasury: TREASURY,
        },
    );
    let forward = handle(&init).unwrap().sync_messages.remove(0);
    let created = handle(&ReceiveInput {
        receiver: config_actor,
        from: Some(init.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: forward.message,
    })
    .unwrap();
    let config = created.post_state.unwrap();
    let boot = created.sync_messages.into_iter().next().unwrap();
    assert!(
        boot.pda_seeds
            .contains(&curve_core::compute_admin_role_seed(NAMESPACE))
    );
    let proposed = authority_call(boot, config_actor, ActorState::empty(), true).unwrap();
    let consent = authority_call(
        proposed.sync_messages.into_iter().next().unwrap(),
        role_actor,
        ActorState::empty(),
        true,
    )
    .unwrap();
    let installed = authority_call(
        consent.sync_messages.into_iter().next().unwrap(),
        Actor::new(NAMESPACE, AUTHORITY),
        proposed.post_state.unwrap(),
        false,
    )
    .unwrap();
    let role_state = installed.post_state.unwrap();
    let response = admin_change(
        config.clone(),
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: successor,
            candidate: AdminCandidate::Pda {
                program_id: owner_program,
                seed: [82; 32],
            },
        },
    );
    let transfer = authority_call(
        response.sync_messages.into_iter().next().unwrap(),
        config_actor,
        ActorState::empty(),
        true,
    )
    .unwrap();
    let proposed = authority_call(
        transfer.sync_messages.into_iter().next().unwrap(),
        Actor::new(NAMESPACE, AUTHORITY),
        role_state,
        false,
    )
    .unwrap();
    let request = proposed.sync_messages.into_iter().next().unwrap();
    assert_eq!(request.to, Actor::new(successor, owner_program));
    let owner_input = ReceiveInput {
        receiver: request.to,
        from: Some(role_actor),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: request.message,
    };
    let approval = lez_compat_core::authority::approve_pda(&owner_input, AUTHORITY)
        .unwrap()
        .sync_messages
        .remove(0);
    assert!(approval.pda_seeds.contains(&seed));
    assert!(
        authority_call(
            approval.clone(),
            owner_input.receiver,
            ActorState::empty(),
            false
        )
        .is_err()
    );
    let accepted =
        authority_call(approval, owner_input.receiver, ActorState::empty(), true).unwrap();
    let installed = authority_call(
        accepted.sync_messages.into_iter().next().unwrap(),
        Actor::new(successor, AUTHORITY),
        proposed.post_state.unwrap(),
        false,
    )
    .unwrap();
    let signed = input(
        successor,
        ActorState::empty(),
        true,
        Message::UpdateFee {
            namespace: NAMESPACE,
            protocol_fee_bps: 77,
        },
    );
    let forward = handle(&signed).unwrap().sync_messages.remove(0);
    let pending = handle(&ReceiveInput {
        receiver: forward.to,
        from: Some(signed.receiver),
        is_authorized: false,
        pre_state: config,
        message: forward.message,
    })
    .unwrap();
    let check = authority_call(
        pending.sync_messages.into_iter().next().unwrap(),
        config_actor,
        ActorState::empty(),
        true,
    )
    .unwrap();
    let proof = authority_call(
        check.sync_messages.into_iter().next().unwrap(),
        Actor::new(successor, AUTHORITY),
        installed.post_state.unwrap(),
        false,
    )
    .unwrap()
    .sync_messages
    .remove(0);
    let updated = handle(&ReceiveInput {
        receiver: proof.to,
        from: Some(role_actor),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: proof.message,
    })
    .unwrap();
    assert_eq!(
        Config::decode(&updated.post_state.unwrap())
            .unwrap()
            .protocol_fee_bps,
        77
    );
}

#[test]
fn cosigned_transfer_removes_old_admin_and_renunciation_is_terminal() {
    use curve_core::{AUTHORITY_PROGRAM_ACCOUNT_ID as AUTHORITY, compute_admin_role};
    use lee_core::program::SyncMessage;
    use lez_compat_core::authority::Message as RoleMessage;
    let role = Actor::new(compute_admin_role(NAMESPACE, PROGRAM), AUTHORITY);
    let current = Actor::new(NAMESPACE, AUTHORITY);
    let successor = Actor::new(AccountId::new([2; 32]), AUTHORITY);
    let boot = authority_call(
        SyncMessage::new(
            role,
            &RoleMessage::Bootstrap {
                authority: NAMESPACE,
            },
        ),
        Actor::new(compute_config_pda(NAMESPACE, PROGRAM), PROGRAM),
        ActorState::empty(),
        true,
    )
    .unwrap();
    let consent = authority_call(
        boot.sync_messages.into_iter().next().unwrap(),
        role,
        ActorState::empty(),
        true,
    )
    .unwrap();
    let installed = authority_call(
        consent.sync_messages.into_iter().next().unwrap(),
        current,
        boot.post_state.unwrap(),
        false,
    )
    .unwrap();
    let before = namespace_state();
    let transfer = admin_change(
        before.clone(),
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: successor.account_id,
            candidate: curve_core::AdminCandidate::Signer,
        },
    );
    assert_eq!(transfer.post_state.unwrap(), before);
    let transfer = authority_call(
        transfer.sync_messages.into_iter().next().unwrap(),
        Actor::new(compute_config_pda(NAMESPACE, PROGRAM), PROGRAM),
        ActorState::empty(),
        true,
    )
    .unwrap();
    let proposed = authority_call(
        transfer.sync_messages.into_iter().next().unwrap(),
        current,
        installed.post_state.unwrap(),
        false,
    )
    .unwrap();
    let consent = proposed.sync_messages.into_iter().next().unwrap();
    assert!(authority_call(consent.clone(), role, ActorState::empty(), false).is_err());
    let accepted = authority_call(consent, role, ActorState::empty(), true).unwrap();
    let installed = authority_call(
        accepted.sync_messages.into_iter().next().unwrap(),
        successor,
        proposed.post_state.unwrap(),
        false,
    )
    .unwrap();
    let check = |actor: Actor, state: ActorState| {
        authority_call(
            SyncMessage::new(
                role,
                &RoleMessage::Assert {
                    authority: actor.account_id,
                    callback: Actor::new(compute_config_pda(NAMESPACE, PROGRAM), PROGRAM),
                },
            ),
            actor,
            state,
            false,
        )
    };
    let role_state = installed.post_state.unwrap();
    assert!(check(current, role_state.clone()).is_err());
    assert!(check(successor, role_state.clone()).is_ok());
    let revoke = authority_call(
        SyncMessage::new(
            successor,
            &RoleMessage::Renounce {
                role: role.account_id,
            },
        ),
        Actor::new(successor.account_id, PROGRAM),
        ActorState::empty(),
        true,
    )
    .unwrap();
    let revoked = authority_call(
        revoke.sync_messages.into_iter().next().unwrap(),
        successor,
        role_state,
        false,
    )
    .unwrap()
    .post_state
    .unwrap();
    assert!(check(successor, revoked.clone()).is_err());
    assert!(check(current, revoked.clone()).is_err());
    assert!(
        authority_call(
            SyncMessage::new(
                role,
                &RoleMessage::Bootstrap {
                    authority: NAMESPACE
                }
            ),
            current,
            revoked,
            true
        )
        .is_err()
    );
}

#[test]
fn namespace_creation_requires_its_signer_valid_settings_and_empty_canonical_state() {
    let message = Message::InitializeNamespace {
        namespace: NAMESPACE,
        protocol_fee_bps: 500,
        treasury: TREASURY,
    };
    assert!(
        handle(&input(
            NAMESPACE,
            ActorState::empty(),
            false,
            message.clone()
        ))
        .is_err()
    );
    let mut wrong = input(
        AccountId::new([9; 32]),
        ActorState::empty(),
        true,
        message.clone(),
    );
    assert!(handle(&wrong).is_err());
    wrong.receiver.account_id = compute_config_pda(NAMESPACE, PROGRAM);
    wrong.from = Some(Actor::new(AccountId::new([9; 32]), PROGRAM));
    assert!(handle(&wrong).is_err());
    wrong.from = Some(Actor::new(NAMESPACE, PROGRAM));
    wrong.pre_state = namespace_state();
    assert!(handle(&wrong).is_err());
    for (namespace, fee, treasury) in [
        (AccountId::default(), 500, TREASURY),
        (NAMESPACE, 10000, TREASURY),
        (NAMESPACE, u32::MAX, TREASURY),
        (NAMESPACE, 500, AccountId::default()),
    ] {
        assert!(
            handle(&input(
                namespace,
                ActorState::empty(),
                true,
                Message::InitializeNamespace {
                    namespace,
                    protocol_fee_bps: fee,
                    treasury
                }
            ))
            .is_err()
        );
    }
    for fee in [0, 9999] {
        assert!(
            handle(&input(
                NAMESPACE,
                ActorState::empty(),
                true,
                Message::InitializeNamespace {
                    namespace: NAMESPACE,
                    protocol_fee_bps: fee,
                    treasury: TREASURY
                }
            ))
            .is_ok()
        );
    }
}

#[test]
fn controls_reject_legacy_corrupt_or_unknown_version_namespace_state() {
    let original = namespace_state();
    let mut unknown = original.to_vec();
    unknown[8..10].copy_from_slice(&4u16.to_le_bytes());
    let mut corrupt = original.to_vec();
    corrupt[0] ^= 1;
    for state in [
        ActorState::empty(),
        vec![0; 64].into(),
        unknown.into(),
        corrupt.into(),
        original[..original.len() - 1].to_vec().into(),
    ] {
        for message in [
            Message::ReadNamespace {
                namespace: NAMESPACE,
            },
            Message::UpdateFee {
                namespace: NAMESPACE,
                protocol_fee_bps: 1,
            },
            Message::SetFrozen {
                namespace: NAMESPACE,
                frozen: true,
            },
        ] {
            let mut request = input(
                compute_config_pda(NAMESPACE, PROGRAM),
                state.clone(),
                false,
                message,
            );
            request.from = Some(Actor::new(NAMESPACE, PROGRAM));
            assert!(handle(&request).is_err());
        }
    }
}

#[test]
fn admin_changes_preserve_emergency_control_and_validate_fee_treasury_and_successor() {
    let emergency = input(
        INITIAL_EMERGENCY_AUTHORITY,
        ActorState::empty(),
        true,
        Message::TransferEmergencyAuthority {
            namespace: NAMESPACE,
            new_authority: AccountId::new([102; 32]),
        },
    );
    let forward = handle(&emergency).unwrap().sync_messages.remove(0);
    let transferred = handle(&ReceiveInput {
        receiver: forward.to,
        from: Some(emergency.receiver),
        is_authorized: false,
        pre_state: namespace_state(),
        message: forward.message,
    })
    .unwrap();
    let mut config = Config::decode(&transferred.post_state.unwrap()).unwrap();
    assert_eq!(config.emergency.authority(), AccountId::new([102; 32]));
    config.emergency.is_frozen = true;
    let frozen: ActorState = borsh::to_vec(&config).unwrap().into();
    for fee in [0, 9999] {
        let result = admin_change(
            frozen.clone(),
            Message::UpdateFee {
                namespace: NAMESPACE,
                protocol_fee_bps: fee,
            },
        );
        let updated = Config::decode(&result.post_state.unwrap()).unwrap();
        assert_eq!(updated.protocol_fee_bps, fee as u16);
        assert_eq!(updated.emergency, config.emergency);
        assert_eq!(updated.treasury, config.treasury);
    }
    for message in [
        Message::UpdateFee {
            namespace: NAMESPACE,
            protocol_fee_bps: 10000,
        },
        Message::UpdateFee {
            namespace: NAMESPACE,
            protocol_fee_bps: u32::MAX,
        },
        Message::UpdateTreasury {
            namespace: NAMESPACE,
            treasury: AccountId::default(),
        },
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: AccountId::default(),
            candidate: curve_core::AdminCandidate::Signer,
        },
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: AccountId::new([9; 32]),
            candidate: curve_core::AdminCandidate::Pda {
                program_id: PROGRAM,
                seed: [9; 32],
            },
        },
    ] {
        assert!(admin_change_result(frozen.clone(), message).is_err());
    }
    for message in [
        Message::UpdateTreasury {
            namespace: NAMESPACE,
            treasury: AccountId::new([8; 32]),
        },
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: AccountId::new([9; 32]),
            candidate: curve_core::AdminCandidate::Signer,
        },
        Message::RenounceAdmin {
            namespace: NAMESPACE,
        },
    ] {
        let updated =
            Config::decode(&admin_change(frozen.clone(), message).post_state.unwrap()).unwrap();
        assert_eq!(updated.emergency, config.emergency);
    }
    config.protocol_fee_bps = 10000;
    let corrected = admin_change(
        borsh::to_vec(&config).unwrap().into(),
        Message::UpdateFee {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
        },
    );
    assert_eq!(
        Config::decode(&corrected.post_state.unwrap())
            .unwrap()
            .protocol_fee_bps,
        500
    );
}

#[test]
fn every_admin_action_requires_its_current_namespace_role_even_for_no_op_changes() {
    let messages = [
        Message::UpdateFee {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
        },
        Message::UpdateTreasury {
            namespace: NAMESPACE,
            treasury: TREASURY,
        },
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: NAMESPACE,
            candidate: curve_core::AdminCandidate::Signer,
        },
        Message::RenounceAdmin {
            namespace: NAMESPACE,
        },
    ];
    for message in messages {
        assert!(
            handle(&input(
                NAMESPACE,
                ActorState::empty(),
                false,
                message.clone()
            ))
            .is_err()
        );
        let signed = input(NAMESPACE, ActorState::empty(), true, message);
        let forward = handle(&signed).unwrap().sync_messages.remove(0);
        let pending = handle(&ReceiveInput {
            receiver: forward.to,
            from: Some(signed.receiver),
            is_authorized: false,
            pre_state: namespace_state(),
            message: forward.message,
        })
        .unwrap();
        let current = curve_core::compute_admin_role(NAMESPACE, PROGRAM);
        for (role, authority) in [
            (
                curve_core::compute_admin_role(AccountId::new([2; 32]), PROGRAM),
                NAMESPACE,
            ),
            (current, AccountId::new([9; 32])),
        ] {
            assert!(
                handle(&ReceiveInput {
                    receiver: forward.to,
                    from: Some(Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)),
                    is_authorized: false,
                    pre_state: pending.post_state.clone().unwrap(),
                    message: borsh::to_vec(&lez_compat_core::authority::Authorized {
                        role,
                        authority
                    })
                    .unwrap()
                })
                .is_err()
            );
        }
    }
}

fn namespace_state() -> ActorState {
    let signer = input(
        NAMESPACE,
        ActorState::empty(),
        true,
        Message::InitializeNamespace {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
            treasury: TREASURY,
        },
    );
    let call = handle(&signer).unwrap().sync_messages.remove(0);
    handle(&ReceiveInput {
        receiver: call.to,
        from: Some(signer.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: call.message,
    })
    .unwrap()
    .post_state
    .unwrap()
}
fn admin_change(state: ActorState, message: Message) -> lee_core::program::Response {
    admin_change_result(state, message).unwrap()
}
fn admin_change_result(
    state: ActorState,
    message: Message,
) -> Result<lee_core::program::Response, curve_core::Error> {
    let signer = input(NAMESPACE, ActorState::empty(), true, message);
    let call = handle(&signer).unwrap().sync_messages.remove(0);
    let pending = handle(&ReceiveInput {
        receiver: call.to,
        from: Some(signer.receiver),
        is_authorized: false,
        pre_state: state,
        message: call.message,
    })
    .unwrap();
    let role = curve_core::compute_admin_role(NAMESPACE, PROGRAM);
    handle(&ReceiveInput {
        receiver: call.to,
        from: Some(Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: borsh::to_vec(&lez_compat_core::authority::Authorized {
            role,
            authority: NAMESPACE,
        })
        .unwrap(),
    })
}

#[test]
fn admin_transfer_uses_the_successor_consent_protocol_and_preserves_config() {
    let before = namespace_state();
    let response = admin_change(
        before.clone(),
        Message::TransferAdmin {
            namespace: NAMESPACE,
            new_admin: AccountId::new([2; 32]),
            candidate: curve_core::AdminCandidate::Signer,
        },
    );
    assert_eq!(response.post_state.unwrap(), before);
    let call = &response.sync_messages[0];
    assert_eq!(
        call.to,
        Actor::new(NAMESPACE, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
    );
    let message: lez_compat_core::authority::Message = borsh::from_slice(&call.message).unwrap();
    assert_eq!(
        message,
        lez_compat_core::authority::Message::Transfer {
            role: curve_core::compute_admin_role(NAMESPACE, PROGRAM),
            successor: AccountId::new([2; 32])
        }
    );
}

#[test]
fn a_namespace_query_returns_only_the_requested_canonical_config() {
    let state = namespace_state();
    let mut query = input(
        curve_core::compute_config_pda(NAMESPACE, PROGRAM),
        state.clone(),
        false,
        Message::ReadNamespace {
            namespace: NAMESPACE,
        },
    );
    query.from = Some(Actor::new(AccountId::new([9; 32]), AccountId::new([8; 32])));
    let reply = handle(&query).unwrap().sync_messages.remove(0);
    let decoded: lee_core::program::StateReply = borsh::from_slice(&reply.message).unwrap();
    assert_eq!(decoded.state, state);
    assert_eq!(reply.to, query.from.unwrap());
    query.receiver.account_id = AccountId::new([99; 32]);
    assert!(handle(&query).is_err());
}

#[test]
fn renunciation_leaves_namespace_settings_and_revokes_the_separate_admin_role() {
    let before = namespace_state();
    let response = admin_change(
        before.clone(),
        Message::RenounceAdmin {
            namespace: NAMESPACE,
        },
    );
    assert_eq!(response.post_state.unwrap(), before);
    let message: lez_compat_core::authority::Message =
        borsh::from_slice(&response.sync_messages[0].message).unwrap();
    assert_eq!(
        message,
        lez_compat_core::authority::Message::Renounce {
            role: curve_core::compute_admin_role(NAMESPACE, PROGRAM)
        }
    );
}
