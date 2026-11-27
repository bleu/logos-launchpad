use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::ReceiveInput,
};
use lez_compat_core::authority::{self, Message};

const PROGRAM: AccountId = AccountId::new([8; 32]);
const ROLE: AccountId = AccountId::new([1; 32]);
const ALICE: AccountId = AccountId::new([2; 32]);

fn input(
    account: AccountId,
    state: ActorState,
    authorized: bool,
    message: Message,
) -> ReceiveInput {
    ReceiveInput {
        receiver: Actor::new(account, PROGRAM),
        from: None,
        is_authorized: authorized,
        pre_state: state,
        message: borsh::to_vec(&message).unwrap(),
    }
}

#[test]
fn an_authorized_role_creation_installs_a_cosigning_authority() {
    let creation = input(
        ROLE,
        ActorState::empty(),
        true,
        Message::Bootstrap { authority: ALICE },
    );
    let response = authority::handle(&creation).unwrap();
    let challenge = &response.sync_messages[0];
    let approved = authority::handle(&ReceiveInput {
        receiver: challenge.to,
        from: Some(creation.receiver),
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: challenge.message.clone(),
    })
    .unwrap();
    let commit = &approved.sync_messages[0];
    let installed = authority::handle(&ReceiveInput {
        receiver: commit.to,
        from: Some(challenge.to),
        is_authorized: false,
        pre_state: response.post_state.unwrap(),
        message: commit.message.clone(),
    })
    .unwrap();
    let request = input(
        ALICE,
        ActorState::empty(),
        true,
        Message::Check {
            role: ROLE,
            callback: Actor::new(ALICE, PROGRAM),
        },
    );
    let check = authority::handle(&request).unwrap().sync_messages.remove(0);
    assert!(
        authority::handle(&ReceiveInput {
            receiver: check.to,
            from: Some(request.receiver),
            is_authorized: false,
            pre_state: installed.post_state.unwrap(),
            message: check.message
        })
        .is_ok()
    );
}

fn bootstrap() -> ActorState {
    bootstrap_with(Message::Bootstrap { authority: ALICE })
}

fn bootstrap_with(message: Message) -> ActorState {
    let creation = input(ROLE, ActorState::empty(), true, message);
    let response = authority::handle(&creation).unwrap();
    let challenge = &response.sync_messages[0];
    let approved = authority::handle(&ReceiveInput {
        receiver: challenge.to,
        from: Some(creation.receiver),
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: challenge.message.clone(),
    })
    .unwrap();
    let commit = &approved.sync_messages[0];
    authority::handle(&ReceiveInput {
        receiver: commit.to,
        from: Some(challenge.to),
        is_authorized: false,
        pre_state: response.post_state.unwrap(),
        message: commit.message.clone(),
    })
    .unwrap()
    .post_state
    .unwrap()
}

fn check(
    state: ActorState,
    who: AccountId,
) -> Result<lee_core::program::Response, lez_compat_core::Error> {
    let request = input(
        who,
        ActorState::empty(),
        true,
        Message::Check {
            role: ROLE,
            callback: Actor::new(who, PROGRAM),
        },
    );
    let forwarded = authority::handle(&request)?.sync_messages.remove(0);
    authority::handle(&ReceiveInput {
        receiver: forwarded.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: state,
        message: forwarded.message,
    })
}

#[test]
fn transfer_requires_the_current_authority_and_the_successor_signature() {
    transfer_with_consent(bootstrap());
}

#[test]
fn a_transfer_only_role_keeps_its_policy_after_a_consented_transfer() {
    let transferred = transfer_with_consent(bootstrap_with(Message::BootstrapTransferOnly {
        authority: ALICE,
    }));
    let bob = AccountId::new([3; 32]);
    let request = input(
        bob,
        ActorState::empty(),
        true,
        Message::Renounce { role: ROLE },
    );
    let revoke = authority::handle(&request).unwrap().sync_messages.remove(0);
    assert!(matches!(
        authority::handle(&ReceiveInput {
            receiver: revoke.to,
            from: Some(request.receiver),
            is_authorized: false,
            pre_state: transferred.clone(),
            message: revoke.message,
        }),
        Err(lez_compat_core::Error::Unauthorized)
    ));
    assert!(check(transferred, bob).is_ok());
}

fn transfer_with_consent(initial: ActorState) -> ActorState {
    let bob = AccountId::new([3; 32]);
    let request = input(
        ALICE,
        ActorState::empty(),
        true,
        Message::Transfer {
            role: ROLE,
            successor: bob,
        },
    );
    let proposal = authority::handle(&request).unwrap().sync_messages.remove(0);
    let pending = authority::handle(&ReceiveInput {
        receiver: proposal.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: initial,
        message: proposal.message,
    })
    .unwrap();
    let challenge = &pending.sync_messages[0];
    let mut approval = ReceiveInput {
        receiver: challenge.to,
        from: Some(proposal.to),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: challenge.message.clone(),
    };
    assert!(authority::handle(&approval).is_err());
    approval.is_authorized = true;
    let commit = authority::handle(&approval)
        .unwrap()
        .sync_messages
        .remove(0);
    let transferred = authority::handle(&ReceiveInput {
        receiver: commit.to,
        from: Some(approval.receiver),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: commit.message,
    })
    .unwrap()
    .post_state
    .unwrap();
    assert!(check(transferred.clone(), ALICE).is_err());
    assert!(check(transferred.clone(), bob).is_ok());
    transferred
}

#[test]
fn renunciation_is_terminal_and_cannot_reopen_bootstrap() {
    let request = input(
        ALICE,
        ActorState::empty(),
        true,
        Message::Renounce { role: ROLE },
    );
    let forwarded = authority::handle(&request).unwrap().sync_messages.remove(0);
    let renounced = authority::handle(&ReceiveInput {
        receiver: forwarded.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: bootstrap(),
        message: forwarded.message,
    })
    .unwrap()
    .post_state
    .unwrap();
    assert!(check(renounced.clone(), ALICE).is_err());
    assert!(
        authority::handle(&input(
            ROLE,
            renounced,
            true,
            Message::Bootstrap { authority: ALICE }
        ))
        .is_err()
    );
}

#[test]
fn a_transfer_only_role_rejects_renunciation_and_keeps_its_holder() {
    let state = bootstrap_with(Message::BootstrapTransferOnly { authority: ALICE });
    let request = input(
        ALICE,
        ActorState::empty(),
        true,
        Message::Renounce { role: ROLE },
    );
    let revoke = authority::handle(&request).unwrap().sync_messages.remove(0);
    let rejected = authority::handle(&ReceiveInput {
        receiver: revoke.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: state.clone(),
        message: revoke.message,
    });
    assert!(matches!(
        rejected,
        Err(lez_compat_core::Error::Unauthorized)
    ));
    assert!(check(state, ALICE).is_ok());
}

#[test]
fn a_program_owned_successor_accepts_through_its_own_seed_grant() {
    use lee_core::program::PdaSeed;
    let owner_program = AccountId::new([7; 32]);
    let seed = PdaSeed::new([4; 32]);
    let successor = AccountId::for_public_pda(&owner_program, &seed);
    let request = input(
        ALICE,
        ActorState::empty(),
        true,
        Message::TransferPda {
            role: ROLE,
            program: owner_program,
            seed,
        },
    );
    let proposal = authority::handle(&request).unwrap().sync_messages.remove(0);
    let pending = authority::handle(&ReceiveInput {
        receiver: proposal.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: bootstrap(),
        message: proposal.message,
    })
    .unwrap();
    let challenge = &pending.sync_messages[0];
    let granted = authority::approve_pda(
        &ReceiveInput {
            receiver: challenge.to,
            from: Some(proposal.to),
            is_authorized: false,
            pre_state: ActorState::empty(),
            message: challenge.message.clone(),
        },
        PROGRAM,
    )
    .unwrap();
    let acceptance = &granted.sync_messages[0];
    assert!(acceptance.pda_seeds.contains(&seed));
    let mut receipt = ReceiveInput {
        receiver: acceptance.to,
        from: Some(challenge.to),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: acceptance.message.clone(),
    };
    assert!(authority::handle(&receipt).is_err());
    receipt.is_authorized = true;
    let commit = authority::handle(&receipt).unwrap().sync_messages.remove(0);
    let transferred = authority::handle(&ReceiveInput {
        receiver: commit.to,
        from: Some(receipt.receiver),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: commit.message,
    })
    .unwrap()
    .post_state
    .unwrap();
    assert!(check(transferred, successor).is_ok());
}

#[test]
fn a_role_proof_cannot_authorize_another_role_or_program() {
    let response = check(bootstrap(), ALICE).unwrap();
    let callback = &response.sync_messages[0];
    let mut reply = ReceiveInput {
        receiver: callback.to,
        from: Some(Actor::new(ROLE, PROGRAM)),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: callback.message.clone(),
    };
    assert_eq!(
        authority::verify(&reply, Actor::new(ROLE, PROGRAM), ALICE),
        Ok(())
    );
    assert!(
        authority::verify(&reply, Actor::new(AccountId::new([5; 32]), PROGRAM), ALICE).is_err()
    );
    reply.from = Some(Actor::new(ROLE, AccountId::new([6; 32])));
    assert!(authority::verify(&reply, Actor::new(ROLE, PROGRAM), ALICE).is_err());
}

#[test]
fn a_pda_approval_does_not_grant_its_seed_to_another_authority_program() {
    use lee_core::program::PdaSeed;
    let owner = AccountId::new([7; 32]);
    let seed = PdaSeed::new([4; 32]);
    let untrusted_role = Actor::new(ROLE, AccountId::new([6; 32]));
    let request = authority::PdaRequest {
        role: untrusted_role,
        epoch: 0,
        seed,
    };
    let input = ReceiveInput {
        receiver: Actor::new(AccountId::for_public_pda(&owner, &seed), owner),
        from: Some(untrusted_role),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: borsh::to_vec(&request).unwrap(),
    };
    assert!(authority::approve_pda(&input, PROGRAM).is_err());
}

fn resolve(state: ActorState) -> ReceiveInput {
    let caller = Actor::new(AccountId::new([9; 32]), AccountId::new([7; 32]));
    let request = ReceiveInput {
        from: Some(caller),
        ..input(ROLE, state, false, Message::Resolve)
    };
    let response = authority::handle(&request).unwrap();
    assert!(response.post_state.is_none());
    assert_eq!(response.sync_messages.len(), 1);
    let callback = &response.sync_messages[0];
    assert_eq!(callback.to, caller);
    assert!(callback.pda_seeds.is_empty());
    ReceiveInput {
        receiver: callback.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: callback.message.clone(),
    }
}

#[test]
fn unsigned_resolution_returns_the_current_holder_to_the_calling_actor() {
    let expected = Actor::new(ROLE, PROGRAM);
    let initial = bootstrap();
    assert_eq!(authority::current_holder(&initial), Ok(ALICE));
    assert_eq!(
        authority::verify_resolved(&resolve(initial), expected),
        Ok(ALICE)
    );
    let transferred = transfer_with_consent(bootstrap_with(Message::BootstrapTransferOnly {
        authority: ALICE,
    }));
    let bob = AccountId::new([3; 32]);
    assert_eq!(authority::current_holder(&transferred), Ok(bob));
    assert_eq!(
        authority::verify_resolved(&resolve(transferred), expected),
        Ok(bob)
    );
}

#[test]
fn resolution_verifies_the_full_sender_and_payload_role() {
    let expected = Actor::new(ROLE, PROGRAM);
    let reply = resolve(bootstrap());
    for sender in [
        None,
        Some(Actor::new(AccountId::new([5; 32]), PROGRAM)),
        Some(Actor::new(ROLE, AccountId::new([6; 32]))),
    ] {
        assert_eq!(
            authority::verify_resolved(
                &ReceiveInput {
                    from: sender,
                    ..reply.clone()
                },
                expected
            ),
            Err(lez_compat_core::Error::Unauthorized)
        );
    }
    let wrong_role = ReceiveInput {
        message: borsh::to_vec(&authority::CurrentAuthority {
            role: AccountId::new([5; 32]),
            authority: ALICE,
        })
        .unwrap(),
        ..reply
    };
    assert_eq!(
        authority::verify_resolved(&wrong_role, expected),
        Err(lez_compat_core::Error::Unauthorized)
    );
}

#[test]
fn resolution_rejects_zero_authority_and_malformed_replies() {
    let expected = Actor::new(ROLE, PROGRAM);
    let reply = resolve(bootstrap());
    let zero = ReceiveInput {
        message: borsh::to_vec(&authority::CurrentAuthority {
            role: ROLE,
            authority: AccountId::default(),
        })
        .unwrap(),
        ..reply.clone()
    };
    assert_eq!(
        authority::verify_resolved(&zero, expected),
        Err(lez_compat_core::Error::Unauthorized)
    );
    let mut trailing = reply.message.clone();
    trailing.push(0);
    for message in [Vec::new(), reply.message[..63].to_vec(), trailing] {
        assert_eq!(
            authority::verify_resolved(
                &ReceiveInput {
                    message,
                    ..reply.clone()
                },
                expected
            ),
            Err(lez_compat_core::Error::InvalidMessage)
        );
    }
}

#[test]
fn resolution_requires_a_caller_and_valid_role_state() {
    let request = input(ROLE, bootstrap(), false, Message::Resolve);
    assert!(matches!(
        authority::handle(&request),
        Err(lez_compat_core::Error::Unauthorized)
    ));
    let caller = Actor::new(AccountId::new([9; 32]), PROGRAM);
    let malformed = ReceiveInput {
        from: Some(caller),
        ..input(ROLE, ActorState::empty(), false, Message::Resolve)
    };
    assert!(matches!(
        authority::handle(&malformed),
        Err(lez_compat_core::Error::InvalidState)
    ));
    let mut trailing = bootstrap().into_inner();
    trailing.push(0);
    for malformed in [Vec::new(), trailing] {
        assert_eq!(
            authority::current_holder(&malformed),
            Err(lez_compat_core::Error::InvalidState)
        );
    }
    let creation = input(
        ROLE,
        ActorState::empty(),
        true,
        Message::Bootstrap { authority: ALICE },
    );
    let pending = authority::handle(&creation).unwrap().post_state.unwrap();
    assert_eq!(
        authority::current_holder(&pending),
        Err(lez_compat_core::Error::Renounced)
    );
    let request = ReceiveInput {
        pre_state: pending,
        ..malformed
    };
    assert!(matches!(
        authority::handle(&request),
        Err(lez_compat_core::Error::Renounced)
    ));
}

#[test]
fn a_renounced_role_cannot_resolve_a_holder() {
    let request = input(
        ALICE,
        ActorState::empty(),
        true,
        Message::Renounce { role: ROLE },
    );
    let forwarded = authority::handle(&request).unwrap().sync_messages.remove(0);
    let state = authority::handle(&ReceiveInput {
        receiver: forwarded.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: bootstrap(),
        message: forwarded.message,
    })
    .unwrap()
    .post_state
    .unwrap();
    assert_eq!(
        authority::current_holder(&state),
        Err(lez_compat_core::Error::Renounced)
    );
    let request = ReceiveInput {
        from: Some(Actor::new(ALICE, PROGRAM)),
        ..input(ROLE, state, false, Message::Resolve)
    };
    assert!(matches!(
        authority::handle(&request),
        Err(lez_compat_core::Error::Renounced)
    ));
}

#[test]
fn resolving_a_holder_adds_no_action_rights() {
    let state = bootstrap();
    let reply = resolve(state.clone());
    assert_eq!(
        authority::verify_resolved(&reply, Actor::new(ROLE, PROGRAM)),
        Ok(ALICE)
    );
    let outsider = reply.receiver.account_id;
    let callback = reply.receiver;
    for message in [
        Message::Check {
            role: ROLE,
            callback,
        },
        Message::Transfer {
            role: ROLE,
            successor: outsider,
        },
        Message::Renounce { role: ROLE },
    ] {
        assert!(matches!(
            authority::handle(&input(ALICE, ActorState::empty(), false, message)),
            Err(lez_compat_core::Error::Unauthorized)
        ));
    }
    assert!(matches!(
        check(state.clone(), outsider),
        Err(lez_compat_core::Error::Unauthorized)
    ));
    for message in [
        Message::Assert {
            authority: ALICE,
            callback,
        },
        Message::Propose {
            authority: ALICE,
            successor: outsider,
        },
        Message::Revoke { authority: ALICE },
    ] {
        let request = ReceiveInput {
            from: Some(reply.receiver),
            ..input(ROLE, state.clone(), false, message)
        };
        assert!(matches!(
            authority::handle(&request),
            Err(lez_compat_core::Error::Unauthorized)
        ));
    }
    assert!(check(state, ALICE).is_ok());
}
