use clock_core::{CLOCK_01_PROGRAM_ACCOUNT_ID, CLOCK_PROGRAM_ACCOUNT_IDS, ClockAccountData};
use lee::{
    PublicTransaction, V03State,
    error::LeeError,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use lee_core::{
    account::{Account, AccountId, Data},
    program::ProgramId,
};
use std::{collections::HashMap, path::PathBuf};

const RESULT: AccountId = AccountId::new([71; 32]);
const EXECUTION_TIME: u64 = 1_700_000_009_999;

fn guest() -> Program {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release/clock_probe.bin");
    launchpad_client::load_program(&path).expect("build methods before running guest tests")
}

fn chain(probe: &Program) -> V03State {
    let mut accounts: HashMap<_, _> = CLOCK_PROGRAM_ACCOUNT_IDS
        .into_iter()
        .map(|id| {
            (
                id,
                Account {
                    program_owner: programs::clock().id(),
                    data: Data::try_from(
                        ClockAccountData {
                            block_id: 0,
                            timestamp: 0,
                        }
                        .to_bytes(),
                    )
                    .unwrap(),
                    ..Account::default()
                },
            )
        })
        .collect();
    accounts.insert(
        RESULT,
        Account {
            program_owner: probe.id(),
            ..Account::default()
        },
    );
    V03State::new()
        .with_public_accounts(accounts)
        .with_programs([probe.clone(), programs::clock()])
}

fn execute<I: serde::Serialize>(
    state: &mut V03State,
    program: ProgramId,
    accounts: Vec<AccountId>,
    instruction: I,
) -> Result<(), LeeError> {
    let message = Message::try_new(program, accounts, vec![], instruction).unwrap();
    let witnesses = WitnessSet::for_message(&message, &[]);
    state.transition_from_public_transaction(
        &PublicTransaction::new(message, witnesses),
        2,
        EXECUTION_TIME,
    )
}

fn update_clock(state: &mut V03State, timestamp: u64) {
    execute(
        state,
        programs::clock().id(),
        CLOCK_PROGRAM_ACCOUNT_IDS.to_vec(),
        timestamp,
    )
    .expect("system clock update succeeds");
}

fn observe(state: &mut V03State, program: ProgramId, clock: AccountId) -> Result<(), LeeError> {
    execute(state, program, vec![clock, RESULT], ())
}

fn observed_time(state: &V03State) -> u64 {
    u64::from_le_bytes(
        state
            .get_account_by_id(RESULT)
            .data
            .into_inner()
            .try_into()
            .expect("guest returns one millisecond timestamp"),
    )
}

#[test]
fn guest_reads_canonical_clock_in_milliseconds() {
    let probe = guest();
    let mut state = chain(&probe);
    update_clock(&mut state, 1_700_000_000_123);
    let clock = state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID);

    observe(&mut state, probe.id(), CLOCK_01_PROGRAM_ACCOUNT_ID).unwrap();

    assert_eq!(observed_time(&state), 1_700_000_000_123);
    assert_eq!(state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID), clock);
}

#[test]
fn guest_rejects_a_substituted_clock_without_changing_the_result() {
    let probe = guest();
    let mut state = chain(&probe);
    update_clock(&mut state, 1_700_000_000_123);
    observe(&mut state, probe.id(), CLOCK_01_PROGRAM_ACCOUNT_ID).unwrap();
    let before = state.get_account_by_id(RESULT);
    let substitute = AccountId::new([72; 32]);
    let forged = state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID);
    state = state.with_public_accounts([(substitute, forged)]);

    let result = observe(&mut state, probe.id(), substitute);

    assert!(
        result.is_err(),
        "a copied clock account must not be trusted"
    );
    assert_eq!(state.get_account_by_id(RESULT), before);
}

#[test]
fn guest_rejects_a_missing_clock_input() {
    let probe = guest();
    let mut state = chain(&probe);
    let before = state.get_account_by_id(RESULT);

    let result = execute(&mut state, probe.id(), vec![RESULT], ());

    assert!(result.is_err(), "clock input is required");
    assert_eq!(state.get_account_by_id(RESULT), before);
}

#[test]
fn guest_observes_later_clock_updates_in_the_same_second() {
    let probe = guest();
    let mut state = chain(&probe);
    update_clock(&mut state, 1_700_000_000_123);
    observe(&mut state, probe.id(), CLOCK_01_PROGRAM_ACCOUNT_ID).unwrap();
    assert_eq!(observed_time(&state), 1_700_000_000_123);

    update_clock(&mut state, 1_700_000_000_456);
    let clock = state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID);
    observe(&mut state, probe.id(), CLOCK_01_PROGRAM_ACCOUNT_ID).unwrap();

    assert_eq!(observed_time(&state), 1_700_000_000_456);
    assert_eq!(state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID), clock);
}

#[test]
fn guest_rejects_malformed_canonical_clock_data() {
    let probe = guest();
    let mut state = chain(&probe);
    let before = state.get_account_by_id(RESULT);
    let mut clock = state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID);
    clock.data = vec![0; 7].try_into().unwrap();
    state = state.with_public_accounts([(CLOCK_01_PROGRAM_ACCOUNT_ID, clock.clone())]);

    let result = observe(&mut state, probe.id(), CLOCK_01_PROGRAM_ACCOUNT_ID);

    assert!(result.is_err(), "malformed clock data must be rejected");
    assert_eq!(state.get_account_by_id(RESULT), before);
    assert_eq!(state.get_account_by_id(CLOCK_01_PROGRAM_ACCOUNT_ID), clock);
}
