use eligibility_proof_verifier::verify_for;
use lee::{
    Account, AccountId, Actor, PrivateKey, PublicKey, PublicTransaction, V03State,
    ValidatedStateDiff,
    program::Program,
    public_transaction::{Message, WitnessSet},
};
use std::{borrow::Cow, collections::BTreeMap, time::Instant};

const PUBLIC_ACTION_BUDGET: u64 = 10_000_000;

fn bytes(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn transaction(
    state: &V03State,
    actor: Actor,
    key: Option<&PrivateKey>,
    proof: Vec<u8>,
) -> PublicTransaction {
    let nonce = state.get_account_by_id(actor.account_id).nonce;
    let nonces = if key.is_some() {
        BTreeMap::from([(actor.account_id, nonce)])
    } else {
        BTreeMap::new()
    };
    let message = Message::try_new(actor, vec![actor], nonces, proof).unwrap();
    let keys = key.into_iter().collect::<Vec<_>>();
    let witnesses = WitnessSet::for_message(&message, &keys);
    PublicTransaction::new(message, witnesses)
}

#[test]
fn approved_public_trader_fits_the_charged_budget_without_consuming_membership() {
    let members = std::env::var("ELIGIBILITY_MEMBERS").unwrap_or_else(|_| "3".into());
    assert!(members == "3" || members == "100000");
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture_bytes =
        std::fs::read(directory.join(format!("fixtures/merkle-{members}.json"))).unwrap();
    let fixture: serde_json::Value = serde_json::from_slice(&fixture_bytes).unwrap();
    let policy = bytes(fixture["policyHex"].as_str().unwrap());
    let proof = bytes(fixture["proofHex"].as_str().unwrap());
    let guest = Program::new(Cow::Borrowed(eligibility_lite_methods::ELIGIBILITY_ELF)).unwrap();
    let program = AccountId::from_builtin_program(guest.id());
    let key = PrivateKey::try_new([1; 32]).unwrap();
    let other_key = PrivateKey::try_new([2; 32]).unwrap();
    let owner = AccountId::from(&PublicKey::new_from_private_key(&key));
    let other = AccountId::from(&PublicKey::new_from_private_key(&other_key));
    assert_eq!(
        owner.to_bytes().as_slice(),
        bytes(fixture["accountHex"].as_str().unwrap())
    );
    let actor = Actor::new(owner, program);
    let other_actor = Actor::new(other, program);
    let mut state = V03State::new()
        .with_programs([guest])
        .with_public_accounts([
            (
                owner,
                Account::default().with_actor_state(program, policy.clone().into()),
            ),
            (
                other,
                Account::default().with_actor_state(program, policy.clone().into()),
            ),
        ]);
    let host_start = Instant::now();
    verify_for(&policy, &owner.to_bytes(), &proof).unwrap();
    let host_seconds = host_start.elapsed().as_secs_f64();
    let original = transaction(&state, actor, Some(&key), proof.clone());
    let nonce = state.get_account_by_id(owner).nonce;
    let started = Instant::now();
    let (diff, charge) = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &original,
        &state,
        1,
        0,
        PUBLIC_ACTION_BUDGET,
    )
    .expect("membership must finish within the unchanged charged public-action budget");
    let wall_seconds = started.elapsed().as_secs_f64();
    assert!(charge.cycles < PUBLIC_ACTION_BUDGET);
    state.apply_state_diff(diff).unwrap();
    assert_ne!(state.get_account_by_id(owner).nonce, nonce);
    assert_eq!(
        state
            .get_account_by_id(owner)
            .data
            .actor_state(program)
            .as_ref(),
        policy.as_slice()
    );
    assert!(
        ValidatedStateDiff::from_public_transaction_with_cycle_budget(
            &original,
            &state,
            1,
            0,
            PUBLIC_ACTION_BUDGET
        )
        .is_err()
    );

    let mut changed_proof = proof.clone();
    changed_proof[36] ^= 1;
    let mut trailing = proof.clone();
    trailing.push(0);
    let mut changed_signature = transaction(&state, actor, Some(&key), proof.clone());
    changed_signature.message =
        transaction(&state, actor, Some(&key), changed_proof.clone()).message;
    for rejected in [
        changed_signature,
        transaction(&state, other_actor, Some(&other_key), proof.clone()),
        transaction(&state, actor, None, proof.clone()),
        transaction(&state, actor, Some(&key), changed_proof),
        transaction(&state, actor, Some(&key), trailing),
    ] {
        let before = state.get_account_by_id(owner);
        assert!(
            ValidatedStateDiff::from_public_transaction_with_cycle_budget(
                &rejected,
                &state,
                1,
                0,
                PUBLIC_ACTION_BUDGET
            )
            .is_err()
        );
        assert_eq!(state.get_account_by_id(owner), before);
    }
    for offset in [1, 33, 65] {
        let mut wrong_policy = policy.clone();
        wrong_policy[offset] ^= 1;
        let wrong_state = V03State::new()
            .with_programs([
                Program::new(Cow::Borrowed(eligibility_lite_methods::ELIGIBILITY_ELF)).unwrap(),
            ])
            .with_public_accounts([(
                owner,
                Account::default().with_actor_state(program, wrong_policy.into()),
            )]);
        let rejected = transaction(&wrong_state, actor, Some(&key), proof.clone());
        assert!(
            ValidatedStateDiff::from_public_transaction_with_cycle_budget(
                &rejected,
                &wrong_state,
                1,
                0,
                PUBLIC_ACTION_BUDGET
            )
            .is_err()
        );
    }
    let next = transaction(&state, actor, Some(&key), proof);
    let nonce = state.get_account_by_id(owner).nonce;
    let (diff, _) = ValidatedStateDiff::from_public_transaction_with_cycle_budget(
        &next,
        &state,
        1,
        0,
        PUBLIC_ACTION_BUDGET,
    )
    .unwrap();
    state.apply_state_diff(diff).unwrap();
    assert_ne!(state.get_account_by_id(owner).nonce, nonce);

    let result = serde_json::json!({
        "members": fixture["members"], "depth": 17, "budget": PUBLIC_ACTION_BUDGET,
        "transactionActionCycles": charge.cycles, "guestExecutionWallSeconds": wall_seconds,
        "rustHostVerificationSeconds": host_seconds,
        "scope": "synthetic signed self-execution; no sale, fees, private proof or block admission"
    });
    std::fs::create_dir_all(directory.join("results")).unwrap();
    std::fs::write(
        directory.join(format!("results/merkle-{members}-lez.json")),
        format!("{result:#}\n"),
    )
    .unwrap();
    eprintln!("{result:#}");
}
