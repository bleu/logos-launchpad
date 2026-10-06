use event_probe::{ProbeInstruction, SaleOperation, decode};
use lee::{
    PrivateKey, PublicKey, PublicTransaction, V03State, program::Program, public_transaction,
};
use lee_core::account::{AccountId, Nonce, ProgramShardSelector};

#[test]
fn signed_chained_failure_discards_events_and_metered_failure_only_consumes_nonce() {
    let program = Program::new(event_probe_methods::SALE_EVENT_ELF.into()).unwrap();
    let emitter = AccountId::from_builtin_program(program.id());
    let key = PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&PublicKey::new_from_private_key(&key));
    let mut state = V03State::new()
        .with_public_account_balances([(signer, 100)])
        .with_programs([program]);
    let event = || SaleOperation {
        version: 1,
        namespace: [7; 32],
        sale: [8; 32],
        operation: 2,
    };
    let signed_transaction = |nonce, instruction| {
        let message = public_transaction::Message::try_new(
            emitter,
            vec![ProgramShardSelector::native_balance(signer)],
            vec![Nonce(nonce)],
            instruction,
        )
        .unwrap();
        let witnesses = public_transaction::WitnessSet::for_message(&message, &[&key]);
        PublicTransaction::new(message, witnesses)
    };
    let success = signed_transaction(0, ProbeInstruction::Emit(event()));
    let events = state
        .transition_from_public_transaction(&success, 1, 0)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].account_id, emitter);
    assert_eq!(events[0].event.selector, *b"SALEOP01");
    assert_eq!(decode(&events[0].event.data).unwrap(), event());
    assert_eq!(state.get_account_by_id(signer).nonce, Nonce(1));

    let failed = signed_transaction(1, ProbeInstruction::EmitThenFail(event()));
    let before_failure = state.clone();
    assert!(matches!(
        state.transition_from_public_transaction(&failed, 2, 0),
        Err(lee::error::LeeError::ProgramExitedWithCode { code: 7, .. })
    ));
    assert!(state == before_failure);

    let budget = lee::program::DEFAULT_PUBLIC_CYCLE_BUDGET;
    let (charge, diff) =
        lee::ValidatedStateDiff::from_public_transaction_metered(&failed, &state, 2, 0, budget);
    assert!(charge.cycles > 0 && charge.cycles < budget);
    assert!(state.apply_state_diff(diff.unwrap()).is_empty());

    // This runtime reports the charge; fee collection is outside this call.
    let mut expected_signer = before_failure.get_account_by_id(signer);
    expected_signer.nonce = Nonce(2);
    let expected = before_failure.with_public_accounts([(signer, expected_signer)]);
    assert!(state == expected, "only the signer's nonce may change");
    let (replay_charge, replay) =
        lee::ValidatedStateDiff::from_public_transaction_metered(&failed, &state, 2, 0, budget);
    assert!(matches!(replay, Err(lee::error::LeeError::InvalidInput(_))));
    assert_eq!(replay_charge.cycles, 0);
    assert!(state == expected);
}

#[test]
fn failed_public_call_discards_events_emitted_before_chained_failure() {
    let program = Program::new(event_probe_methods::SALE_EVENT_ELF.into()).unwrap();
    let emitter = AccountId::from_builtin_program(program.id());
    let mut state = V03State::new().with_programs([program]);
    let before = borsh::to_vec(&state).unwrap();
    let message = public_transaction::Message::try_new(
        emitter,
        vec![ProgramShardSelector::native_balance(AccountId::new(
            [9; 32],
        ))],
        vec![],
        ProbeInstruction::EmitThenFail(SaleOperation {
            version: 1,
            namespace: [7; 32],
            sale: [8; 32],
            operation: 2,
        }),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&message, &[]);
    let transaction = PublicTransaction::new(message, witnesses);
    let error = state
        .transition_from_public_transaction(&transaction, 1, 0)
        .unwrap_err();
    assert!(matches!(
        error,
        lee::error::LeeError::ProgramExitedWithCode { code: 7, .. }
    ));
    assert_eq!(borsh::to_vec(&state).unwrap(), before);
    let (charge, diff) = lee::ValidatedStateDiff::from_public_transaction_metered(
        &transaction,
        &state,
        1,
        0,
        lee::program::DEFAULT_PUBLIC_CYCLE_BUDGET,
    );
    assert!(charge.cycles > 0);
    assert!(state.apply_state_diff(diff.unwrap()).is_empty());
    assert_eq!(borsh::to_vec(&state).unwrap(), before);
}

#[test]
fn decoder_rejects_unsupported_schema_and_truncated_public_payloads() {
    let program = Program::new(event_probe_methods::SALE_EVENT_ELF.into()).unwrap();
    let emitter = AccountId::from_builtin_program(program.id());
    let mut state = V03State::new().with_programs([program]);
    for version in [2, 1] {
        let message = public_transaction::Message::try_new(
            emitter,
            vec![ProgramShardSelector::native_balance(AccountId::new(
                [9; 32],
            ))],
            vec![],
            ProbeInstruction::Emit(SaleOperation {
                version,
                namespace: [7; 32],
                sale: [8; 32],
                operation: 2,
            }),
        )
        .unwrap();
        let witnesses = public_transaction::WitnessSet::for_message(&message, &[]);
        let events = state
            .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 0)
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event.selector, *b"SALEOP01");
        if version == 2 {
            assert_eq!(
                decode(&events[0].event.data).unwrap_err().kind(),
                std::io::ErrorKind::InvalidData
            );
        } else {
            assert!(decode(&events[0].event.data).is_ok());
            assert!(decode(&events[0].event.data[..65]).is_err());
        }
    }
}

#[test]
fn public_guest_event_preserves_selector_program_and_sale_payload() {
    let program = Program::new(event_probe_methods::SALE_EVENT_ELF.into()).unwrap();
    let emitter = AccountId::from_builtin_program(program.id());
    let mut state = V03State::new().with_programs([program]);
    let message = public_transaction::Message::try_new(
        emitter,
        vec![ProgramShardSelector::native_balance(AccountId::new(
            [9; 32],
        ))],
        vec![],
        ProbeInstruction::Emit(SaleOperation {
            version: 1,
            namespace: [7; 32],
            sale: [8; 32],
            operation: 2,
        }),
    )
    .unwrap();
    let witnesses = public_transaction::WitnessSet::for_message(&message, &[]);
    let events = state
        .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 1, 0)
        .unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].account_id, emitter);
    assert_eq!(events[0].event.selector, *b"SALEOP01");
    let expected_bytes = [vec![1], vec![7; 32], vec![8; 32], vec![2]].concat();
    assert_eq!(events[0].event.data, expected_bytes);
    assert_eq!(
        decode(&events[0].event.data).unwrap(),
        SaleOperation {
            version: 1,
            namespace: [7; 32],
            sale: [8; 32],
            operation: 2,
        }
    );
}
