use std::process::{Command, Stdio};

#[path = "../src/benchmark.rs"]
mod benchmark;

#[test]
fn native_validator_rejects_a_fake_outer_receipt_without_state_change() {
    use eligibility_host::{
        native::{NativeArtifact, TrialState, validate_and_apply},
        native_execution::Trial,
    };
    let input = benchmark::prepare(eligibility_core::TreeHash::Sha256)
        .unwrap()
        .private;
    let trial = Trial::new(input);
    let mut state = TrialState::new(&trial).unwrap();
    let before = state.snapshot();
    let journal = vec![];
    let claim = risc0_zkvm::ReceiptClaim::ok(lee::PRIVACY_PRESERVING_CIRCUIT_ID, journal.clone());
    let receipt = risc0_zkvm::Receipt::new(
        risc0_zkvm::InnerReceipt::Fake(risc0_zkvm::FakeReceipt::new(claim)),
        journal,
    );
    let artifact = NativeArtifact::from_outer_receipt(receipt);
    assert!(validate_and_apply(&mut state, &artifact).is_err());
    assert_eq!(state.snapshot(), before);
}

#[test]
#[ignore = "one bounded real local native proof; private first, public separately"]
fn native_proof_commits_marker_and_mock_purchase_together() {
    use eligibility_core::native::{MockPurchaseState, decode};
    use eligibility_host::{
        native::{TrialState, prove, validate_and_apply},
        native_execution::Trial,
    };
    let prepared = benchmark::prepare(eligibility_core::TreeHash::Sha256).unwrap();
    println!(
        "native fixture account_seconds={:.6} tree_seconds={:.6} membership_seconds={:.6}",
        prepared.account_seconds, prepared.tree_seconds, prepared.membership_seconds
    );
    let account = std::env::var("ELIGIBILITY_NATIVE_ACCOUNT").unwrap_or_else(|_| "private".into());
    let input = match account.as_str() {
        "private" => prepared.private,
        "public" => prepared.public,
        _ => panic!("unsupported synthetic account type"),
    };
    let trial = Trial::new(input);
    let mut state = TrialState::new(&trial).unwrap();
    let before = state.snapshot();
    let artifact =
        prove(&trial).expect("bounded native proof failed; witness details are not logged");
    let transition = validate_and_apply(&mut state, &artifact).unwrap();
    let purchase: MockPurchaseState = decode(&transition.state.purchase.data).unwrap();
    assert_eq!(transition.state.marker.data.as_ref(), [1]);
    assert_eq!(purchase.count, 1);
    assert_eq!(purchase.collateral_in, trial.input.purchase.collateral_in);
    assert_eq!(purchase.tokens_out, trial.input.purchase.tokens_out);
    assert_ne!(transition.state.commitment_root, before.commitment_root);
    assert_eq!(transition.commitments, 1);
    assert_eq!(transition.nullifiers, 1);
    assert!(transition.transaction_bytes < 1_048_576);
    assert_eq!(artifact.artifacts.directory_mode, 0o700);
    assert!(artifact.artifacts.removed);
    if let Some(path) = std::env::var_os("ELIGIBILITY_NATIVE_OUTER_ARTIFACT") {
        use std::{io::Write, os::unix::fs::OpenOptionsExt};
        let bytes: Vec<_> = risc0_zkvm::serde::to_vec(&artifact)
            .unwrap()
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(&bytes).unwrap();
    }
    println!(
        "native account={account} accounts=100000 depth=17 verified=true prove_seconds={:.6} wall_seconds={:.6} sampled_peak_group_rss_bytes={} proof_bytes={} transaction_bytes={} journal_bytes={} commitments={} nullifiers={} sampled_artifact_files_peak={} sampled_artifact_bytes_peak={} artifacts_removed={}",
        artifact.prove_seconds,
        artifact.wall_seconds,
        artifact.sampled_peak_group_rss_bytes,
        transition.proof_bytes,
        transition.transaction_bytes,
        transition.journal_bytes,
        transition.commitments,
        transition.nullifiers,
        artifact.artifacts.files,
        artifact.artifacts.bytes,
        artifact.artifacts.removed
    );
}

#[test]
#[ignore = "reuses a final outer receipt; does not generate another proof"]
fn native_outer_transaction_omits_known_private_input_literals() {
    use eligibility_host::{
        native::{NativeArtifact, TrialState, validate_and_apply},
        native_execution::Trial,
    };
    let path = std::env::var_os("ELIGIBILITY_NATIVE_OUTER_ARTIFACT")
        .expect("set the final outer receipt fixture path");
    let bytes = std::fs::read(path).unwrap();
    let artifact: NativeArtifact = risc0_zkvm::serde::from_slice(&bytes).unwrap();
    let prepared = benchmark::prepare(eligibility_core::TreeHash::Sha256).unwrap();
    let account = std::env::var("ELIGIBILITY_NATIVE_ACCOUNT").unwrap_or_else(|_| "private".into());
    let input = if account == "private" {
        prepared.private
    } else {
        assert_eq!(account, "public");
        prepared.public
    };
    let key = match &input.witness.control {
        eligibility_core::AccountControl::Public { key } => *key,
        eligibility_core::AccountControl::Private { nullifier_key, .. } => *nullifier_key,
    };
    let mut hidden = vec![key, input.witness.membership.blinder];
    hidden.extend(input.witness.membership.siblings.iter().copied());
    for value in [
        "ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8",
        "a5342820e7ab710a41f19c489acf7ac00f2e32fd69a4595428bfb67740ff438e",
    ] {
        hidden.push(hex::decode(value).unwrap().try_into().unwrap());
    }
    let trial = Trial::new(input);
    let mut state = TrialState::new(&trial).unwrap();
    let transition = validate_and_apply(&mut state, &artifact).unwrap();
    let proof = borsh::to_vec(&artifact.receipt.inner).unwrap();
    let mut outputs = vec![
        proof,
        artifact.receipt.journal.bytes,
        transition.transaction,
    ];
    let output_log = std::env::var_os("ELIGIBILITY_NATIVE_OUTPUT_LOG");
    if let Some(path) = &output_log {
        outputs.push(std::fs::read(path).unwrap());
    }
    for (index, value) in hidden.iter().enumerate() {
        let words: Vec<_> = risc0_zkvm::serde::to_vec(value)
            .unwrap()
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect();
        let hexadecimal = hex::encode(value);
        for output in &outputs {
            assert!(
                !output.windows(value.len()).any(|window| window == value),
                "raw private literal index {index} found"
            );
            assert!(
                !output.windows(words.len()).any(|window| window == words),
                "word private literal index {index} found"
            );
            assert!(
                !output
                    .windows(hexadecimal.len())
                    .any(|window| window == hexadecimal.as_bytes()),
                "hex private literal index {index} found"
            );
        }
    }
    println!(
        "native account={account} outer_proof_journal_transaction_literal_scan=true output_log_literal_scan={}",
        output_log.is_some()
    );
}

#[test]
#[ignore = "reuses a final outer receipt; does not generate another proof"]
fn native_validator_rejects_tampering_stale_state_and_replay() {
    use eligibility_host::{
        native::{NativeArtifact, TrialState, validate_and_apply},
        native_execution::Trial,
    };
    let path = std::env::var_os("ELIGIBILITY_NATIVE_OUTER_ARTIFACT")
        .expect("set the final outer receipt fixture path");
    let artifact: NativeArtifact =
        risc0_zkvm::serde::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let prepared = benchmark::prepare(eligibility_core::TreeHash::Sha256).unwrap();
    let account = std::env::var("ELIGIBILITY_NATIVE_ACCOUNT").unwrap_or_else(|_| "private".into());
    let input = if account == "private" {
        prepared.private
    } else {
        assert_eq!(account, "public");
        prepared.public
    };
    let trial = Trial::new(input);
    let mut corrupt = artifact.clone();
    let risc0_zkvm::InnerReceipt::Succinct(ref mut succinct) = corrupt.receipt.inner else {
        panic!("expected real succinct receipt");
    };
    succinct.seal[0] ^= 1;
    let mut changed_journal = artifact.clone();
    changed_journal.receipt.journal.bytes[0] ^= 1;
    let mut changed_purchase = artifact.clone();
    let mut output: lee_core::PrivacyPreservingCircuitOutput =
        changed_purchase.receipt.journal.decode().unwrap();
    let mut purchase: eligibility_core::native::MockPurchaseState =
        eligibility_core::native::decode(&output.public_post_states[2].data).unwrap();
    purchase.tokens_out += 1;
    output.public_post_states[2].data = eligibility_core::native::encode(&purchase)
        .unwrap()
        .try_into()
        .unwrap();
    changed_purchase.receipt.journal.bytes = output.to_bytes();
    for changed in [&corrupt, &changed_journal, &changed_purchase] {
        let mut state = TrialState::new(&trial).unwrap();
        let before = state.snapshot();
        assert!(validate_and_apply(&mut state, changed).is_err());
        assert_eq!(state.snapshot(), before);
        assert!(state.snapshot().marker.data.is_empty());
    }
    let mut stale_trial = trial.clone();
    stale_trial.purchase_account.nonce = lee_core::account::Nonce(1);
    let mut stale = TrialState::new(&stale_trial).unwrap();
    let before = stale.snapshot();
    assert!(validate_and_apply(&mut stale, &artifact).is_err());
    assert_eq!(stale.snapshot(), before);
    assert!(stale.snapshot().marker.data.is_empty());
    let mut changed_policy = trial.clone();
    changed_policy.trusted_policy.collateral_limit += 1;
    let mut state = TrialState::new(&changed_policy).unwrap();
    let before = state.snapshot();
    assert!(validate_and_apply(&mut state, &artifact).is_err());
    assert_eq!(state.snapshot(), before);
    assert!(state.snapshot().marker.data.is_empty());
    let mut state = TrialState::new(&trial).unwrap();
    validate_and_apply(&mut state, &artifact).unwrap();
    let committed = state.snapshot();
    assert!(validate_and_apply(&mut state, &artifact).is_err());
    assert_eq!(state.snapshot(), committed);
    println!(
        "native account={account} corrupt_proof_journal_purchase_rejected=true stale_state_policy_rejected=true replay_rejected=true rejected_state_unchanged=true"
    );
}

#[test]
fn native_proof_command_refuses_remote_prover_before_preparation() {
    let output = Command::new(env!("CARGO_BIN_EXE_eligibility-host"))
        .args(["native-prove", "sha256", "private"])
        .env("RISC0_PROVER", "bonsai")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unsafe native proving configuration\n"
    );
}

#[test]
fn native_worker_refuses_unprotected_storage_before_reading_input() {
    let output = Command::new(env!("CARGO_BIN_EXE_eligibility-host"))
        .arg("native-child")
        .env("TMPDIR", "/tmp")
        .env("RISC0_DEV_MODE", "0")
        .env("RISC0_PROVER", "ipc")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unsafe native worker storage\n"
    );
}

#[test]
fn native_proof_command_refuses_development_proving_before_preparation() {
    let output = Command::new(env!("CARGO_BIN_EXE_eligibility-host"))
        .args(["native-prove", "sha256", "private"])
        .env("RISC0_DEV_MODE", "1")
        .env_remove("RISC0_PROVER")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "unsafe native proving configuration\n"
    );
}
