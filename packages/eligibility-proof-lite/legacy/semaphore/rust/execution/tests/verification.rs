use lee::{
    AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State, ValidatedStateDiff,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use std::{borrow::Cow, collections::BTreeMap, time::Instant};

const PUBLIC_ACTION_BUDGET: u64 = 10_000_000;

#[test]
fn genuine_membership_proof_fits_the_charged_public_action_budget() {
    let fixture = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/proof.json"
    ))
    .expect("generate the official synthetic proof fixture first");
    let guest = Program::new(Cow::Borrowed(eligibility_lite_methods::ELIGIBILITY_ELF)).unwrap();
    let program = AccountId::from_builtin_program(guest.id());
    let key = PrivateKey::try_new([1; 32]).unwrap();
    let owner = AccountId::from(&PublicKey::new_from_private_key(&key));
    let mut state = V03State::new().with_programs([guest]);
    let nonce = state.get_account_by_id(owner).nonce;
    let message = Message::try_new(
        Actor::new(owner, program),
        vec![Actor::new(owner, program)],
        BTreeMap::from([(owner, nonce)]),
        fixture,
    )
    .unwrap();
    let witnesses = WitnessSet::for_message(&message, &[&key]);
    let transaction = PublicTransaction::new(message, witnesses);
    let started = Instant::now();
    let result = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &transaction,
        &state,
        1,
        0,
        PUBLIC_ACTION_BUDGET,
    );
    eprintln!(
        "guest_execution_wall_seconds={:.6}",
        started.elapsed().as_secs_f64()
    );
    let (diff, charge) = match result {
        Ok(result) => result,
        Err(error) => {
            panic!("membership verifier failed the {PUBLIC_ACTION_BUDGET}-cycle gate: {error:?}")
        }
    };
    eprintln!("transaction_action_cycles={}", charge.cycles);
    assert!(charge.cycles < PUBLIC_ACTION_BUDGET);
    state.apply_state_diff(diff).unwrap();
    assert_ne!(state.get_account_by_id(owner).nonce, nonce);
}
