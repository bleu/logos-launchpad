use eligibility_proof_verifier::verify_for;

fn bytes(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn fixture() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let value: serde_json::Value =
        serde_json::from_slice(include_bytes!("../../../fixtures/merkle-vector.json")).unwrap();
    (
        bytes(value["policyHex"].as_str().unwrap()),
        bytes(value["accountHex"].as_str().unwrap()),
        bytes(value["proofHex"].as_str().unwrap()),
    )
}

#[test]
fn approved_public_account_matches_the_independent_sha256_vector() {
    let (policy, account, proof) = fixture();
    assert_eq!(verify_for(&policy, &account, &proof), Ok(()));
    assert_eq!(verify_for(&policy, &account, &proof), Ok(()));
    assert!(verify_for(&policy, &[9; 32], &proof).is_err());
}

#[test]
fn policy_and_path_substitutions_are_rejected() {
    let (policy, account, proof) = fixture();
    for offset in [0, 1, 33, 65] {
        let mut changed = policy.clone();
        changed[offset] ^= 1;
        assert!(verify_for(&changed, &account, &proof).is_err());
    }
    for offset in [0, 3, 4, 35, 36, 68, 579] {
        let mut changed = proof.clone();
        changed[offset] ^= 1;
        assert!(verify_for(&policy, &account, &changed).is_err());
    }
    let mut changed = proof.clone();
    changed[36..68].copy_from_slice(&proof[68..100]);
    changed[68..100].copy_from_slice(&proof[36..68]);
    assert!(verify_for(&policy, &account, &changed).is_err());
}

#[test]
fn encodings_have_exact_lengths_and_a_bounded_index() {
    let (policy, account, proof) = fixture();
    assert!(verify_for(&policy[1..], &account, &proof).is_err());
    assert!(verify_for(&[policy.as_slice(), &[0]].concat(), &account, &proof).is_err());
    assert!(verify_for(&policy, &account[1..], &proof).is_err());
    assert!(verify_for(&policy, &account, &proof[1..]).is_err());
    assert!(verify_for(&policy, &account, &[proof.as_slice(), &[0]].concat()).is_err());
    assert!(verify_for(&policy, &account, &[0; 10000]).is_err());
    let mut changed = proof;
    changed[..4].copy_from_slice(&(1_u32 << 17).to_le_bytes());
    assert!(verify_for(&policy, &account, &changed).is_err());
}
