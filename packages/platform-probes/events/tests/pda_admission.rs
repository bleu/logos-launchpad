use admin_authority::AdminConfig;
use lee::{
    PrivateKey, PublicKey, PublicTransaction, V03State, program::Program, public_transaction,
};
use lee_core::account::{AccountId, Nonce, ProgramShardSelector};
use lee_core::program::PdaSeed;

#[test]
fn runtime_authorized_pda_is_rejected_by_legacy_claim_validation() {
    let admission = Program::new(event_probe_methods::PDA_ADMISSION_ELF.into()).unwrap();
    let caller = Program::new(event_probe_methods::PDA_CALLER_ELF.into()).unwrap();
    let admission_id = AccountId::from_builtin_program(admission.id());
    let caller_id = AccountId::from_builtin_program(caller.id());
    let seed = [9; 32];
    let pda = AccountId::for_public_pda(&caller_id, &PdaSeed::new(seed));
    let key = PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let config = ProgramShardSelector::new(signer, admission_id);
    let mut state = V03State::new()
        .with_public_account_balances([(signer, 100)])
        .with_programs([admission, caller]);

    let message = public_transaction::Message::try_new(
        admission_id,
        vec![ProgramShardSelector::native_balance(signer), config],
        vec![Nonce(0)],
        (0_u8, seed),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&message, &[&key]);
    assert!(
        state
            .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 0)
            .unwrap()
            .is_empty()
    );
    let before = state.get_account_by_id(signer);
    let (admin_before, _, _, result): (Vec<u8>, bool, bool, u8) =
        borsh::from_slice(before.data.shard(admission_id)).unwrap();
    assert_eq!(result, 0);

    let message = public_transaction::Message::try_new(
        caller_id,
        vec![
            ProgramShardSelector::native_balance(signer),
            ProgramShardSelector::new(pda, caller_id),
            ProgramShardSelector::new(signer, admission_id),
        ],
        vec![Nonce(1)],
        (admission_id, seed, 1_u8),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&message, &[&key]);
    assert!(
        state
            .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 2, 0)
            .unwrap()
            .is_empty()
    );

    let after = state.get_account_by_id(signer);
    let (admin_after, current_authorized, pda_authorized, result): (Vec<u8>, bool, bool, u8) =
        borsh::from_slice(after.data.shard(admission_id)).unwrap();
    assert!(current_authorized);
    assert!(pda_authorized);
    assert_eq!(result, 1, "the real library must return UndeployedPda");
    assert_eq!(admin_after, admin_before);
    let admin = AdminConfig::decode(&admin_after).unwrap();
    assert_eq!(admin.current_admin().unwrap().value(), signer.value());
    assert_eq!(
        state.get_account_by_id(pda).data.shard(caller_id).as_ref(),
        &[1_u8]
    );
}

#[test]
fn direct_transfer_without_owner_approval_preserves_the_administrator() {
    let admission = Program::new(event_probe_methods::PDA_ADMISSION_ELF.into()).unwrap();
    let caller = Program::new(event_probe_methods::PDA_CALLER_ELF.into()).unwrap();
    let admission_id = AccountId::from_builtin_program(admission.id());
    let caller_id = AccountId::from_builtin_program(caller.id());
    let seed = [9; 32];
    let pda = AccountId::for_public_pda(&caller_id, &PdaSeed::new(seed));
    let key = PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let mut state = V03State::new()
        .with_public_account_balances([(signer, 100)])
        .with_programs([admission, caller]);
    let initialize = public_transaction::Message::try_new(
        admission_id,
        vec![
            ProgramShardSelector::native_balance(signer),
            ProgramShardSelector::new(signer, admission_id),
        ],
        vec![Nonce(0)],
        (0_u8, seed),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&initialize, &[&key]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(initialize, witnesses), 1, 0)
        .unwrap();
    let before = borsh::to_vec(&state).unwrap();
    let transfer = public_transaction::Message::try_new(
        admission_id,
        vec![
            ProgramShardSelector::native_balance(signer),
            ProgramShardSelector::new(pda, caller_id),
            ProgramShardSelector::new(signer, admission_id),
        ],
        vec![Nonce(1)],
        (2_u8, seed),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&transfer, &[&key]);
    let error = state
        .transition_from_public_transaction(&PublicTransaction::new(transfer, witnesses), 2, 0)
        .unwrap_err();
    assert!(matches!(
        error,
        lee::error::LeeError::ProgramExecutionFailed(_)
    ));
    assert_eq!(borsh::to_vec(&state).unwrap(), before);
}

#[test]
fn owner_program_approval_accepts_the_pda_as_successor() {
    let admission = Program::new(event_probe_methods::PDA_ADMISSION_ELF.into()).unwrap();
    let caller = Program::new(event_probe_methods::PDA_CALLER_ELF.into()).unwrap();
    let admission_id = AccountId::from_builtin_program(admission.id());
    let caller_id = AccountId::from_builtin_program(caller.id());
    let seed = [9; 32];
    let pda = AccountId::for_public_pda(&caller_id, &PdaSeed::new(seed));
    let key = PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let mut state = V03State::new()
        .with_public_account_balances([(signer, 100)])
        .with_programs([admission, caller]);
    let initialize = public_transaction::Message::try_new(
        admission_id,
        vec![
            ProgramShardSelector::native_balance(signer),
            ProgramShardSelector::new(signer, admission_id),
        ],
        vec![Nonce(0)],
        (0_u8, seed),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&initialize, &[&key]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(initialize, witnesses), 1, 0)
        .unwrap();
    let transfer = public_transaction::Message::try_new(
        caller_id,
        vec![
            ProgramShardSelector::native_balance(signer),
            ProgramShardSelector::new(pda, caller_id),
            ProgramShardSelector::new(signer, admission_id),
        ],
        vec![Nonce(1)],
        (admission_id, seed, 2_u8),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&transfer, &[&key]);
    state
        .transition_from_public_transaction(&PublicTransaction::new(transfer, witnesses), 2, 0)
        .unwrap();
    let account = state.get_account_by_id(signer);
    let (bytes, current_authorized, pda_authorized, result): (Vec<u8>, bool, bool, u8) =
        borsh::from_slice(account.data.shard(admission_id)).unwrap();
    assert!(current_authorized && pda_authorized);
    assert_eq!(result, 2);
    assert_eq!(
        AdminConfig::decode(&bytes)
            .unwrap()
            .current_admin()
            .unwrap()
            .value(),
        pda.value(),
    );
}
