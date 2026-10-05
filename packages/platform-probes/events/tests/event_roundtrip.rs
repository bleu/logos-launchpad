use event_probe::{ProbeInstruction, SaleOperation, decode};
use lee::{PublicTransaction, V03State, program::Program, public_transaction};
use lee_core::account::{AccountId, ProgramShardSelector};

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
