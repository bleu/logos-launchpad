use eligibility_core::{
    AccountControl, Digest, EligibilityInput, Membership, Purchase, SaleContext, SalePolicy,
    TreeHash,
};
use eligibility_host::execute;

#[path = "../src/benchmark.rs"]
mod benchmark;

#[test]
#[ignore = "opt-in 100,000-account tree and guest measurement"]
fn guest_checks_large_creator_lists_with_both_account_types_and_hashes() {
    for hash in [TreeHash::Sha256, TreeHash::Poseidon] {
        let prepared = benchmark::prepare(hash).unwrap();
        assert!(prepared.account_seconds >= 0.0);
        assert!(prepared.tree_seconds >= 0.0);
        assert!(prepared.membership_seconds >= 0.0);
        for (input, marker) in [
            (
                &prepared.public,
                "f8bcfd4fa8fefb511c2708862bffc01f0f2267af81c272ef630becd72770db13",
            ),
            (
                &prepared.private,
                "216a87db55c348245382affbee432e0f1a34fa44daa40bac67fa754ca82b76d9",
            ),
        ] {
            assert_eq!(input.policy.depth, 17);
            let result = execute(input).unwrap();
            assert_eq!(result.permit.unwrap().nullifier, digest(marker));
            assert!(result.user_cycles > 0);
        }
    }
}

fn digest(value: &str) -> Digest {
    hex::decode(value).unwrap().try_into().unwrap()
}

fn public_fixture() -> EligibilityInput {
    EligibilityInput {
        policy: SalePolicy {
            sale: SaleContext {
                deployment: [1; 32],
                factory: [2; 32],
                namespace: [3; 32],
                sale_account: [4; 32],
            },
            root: digest("628482ad6a54642544741dded7aa8f0883fa9d7cb2a57301327178a49725af26"),
            hash: TreeHash::Sha256,
            depth: 0,
            collateral_limit: 100,
        },
        purchase: Purchase {
            trading_account: [6; 32],
            recipient: [7; 32],
            collateral_in: 10,
            tokens_out: 20,
        },
        witness: eligibility_core::EligibilityWitness {
            control: AccountControl::Public {
                key: digest("0000000000000000000000000000000000000000000000000000000000000003"),
            },
            membership: Membership {
                blinder: [5; 32],
                index: 0,
                siblings: vec![],
            },
        },
    }
}

#[test]
fn guest_accepts_private_control_and_both_tree_hashes() {
    for (hash, root) in [
        (
            TreeHash::Sha256,
            "d29cf8dc06819098a9703ea750c10f30a9fa19b5f0acdae279b99dd394b419fb",
        ),
        (
            TreeHash::Poseidon,
            "27456fe49f00e4f5b91ba250ac9482d7f1ae7a9448bbf1b32844077d2126ed47",
        ),
    ] {
        let mut input = public_fixture();
        input.policy.hash = hash;
        input.policy.root = digest(root);
        input.witness.control = AccountControl::Private {
            nullifier_key: digest(
                "390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c",
            ),
            identifier: 0,
        };
        let permit = execute(&input).unwrap().permit.unwrap();
        assert_eq!(
            permit.nullifier,
            digest("216a87db55c348245382affbee432e0f1a34fa44daa40bac67fa754ca82b76d9")
        );
    }
}

#[test]
fn guest_rejects_wrong_membership_and_purchase_cap_without_a_permit() {
    for field in 0..3 {
        let mut input = public_fixture();
        match field {
            0 => input.policy.root[0] ^= 1,
            1 => input.witness.membership.blinder[0] ^= 1,
            2 => input.purchase.collateral_in = 101,
            _ => unreachable!(),
        }
        assert!(execute(&input).unwrap().permit.is_none());
    }
}

#[test]
fn verifier_rejects_a_fake_receipt_with_an_otherwise_valid_journal() {
    let input = public_fixture();
    let permit =
        eligibility_core::check_eligibility(&input.policy, &input.purchase, &input.witness)
            .unwrap();
    let words = risc0_zkvm::serde::to_vec(&Some(permit)).unwrap();
    let journal: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
    let claim =
        risc0_zkvm::ReceiptClaim::ok(eligibility_methods::ELIGIBILITY_GUEST_ID, journal.clone());
    let receipt = risc0_zkvm::Receipt::new(
        risc0_zkvm::InnerReceipt::Fake(risc0_zkvm::FakeReceipt::new(claim)),
        journal,
    );
    assert!(eligibility_host::verify_receipt(&receipt, &input.policy, &input.purchase).is_err());
}

#[test]
#[ignore = "opt-in real local proof; run with a wall-time limit"]
fn real_public_receipt_binds_image_journal_and_purchase() {
    let input = public_fixture();
    let evidence =
        eligibility_host::prove(&input, eligibility_host::ReceiptMode::Composite).unwrap();
    let permit =
        eligibility_host::verify_receipt(&evidence.receipt, &input.policy, &input.purchase)
            .unwrap();
    assert_eq!(
        permit.nullifier,
        digest("f8bcfd4fa8fefb511c2708862bffc01f0f2267af81c272ef630becd72770db13")
    );
    assert!(evidence.receipt.verify([0_u32; 8]).is_err());
    let mut wrong_purchase = input.purchase;
    wrong_purchase.recipient[0] ^= 1;
    assert!(
        eligibility_host::verify_receipt(&evidence.receipt, &input.policy, &wrong_purchase)
            .is_err()
    );
    let mut altered = evidence.receipt;
    altered.journal.bytes[4] ^= 1;
    assert!(eligibility_host::verify_receipt(&altered, &input.policy, &input.purchase).is_err());
}

#[test]
fn measurement_command_rejects_bad_arguments_before_proving() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_eligibility-host"))
        .args(["prove", "sha256", "public", "remote"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8(result.stderr).unwrap().contains("usage:"));
}

#[test]
fn measurement_command_reports_metrics_without_witness_data() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_eligibility-host"))
        .args(["execute", "sha256"])
        .output()
        .unwrap();
    assert!(result.status.success());
    let output = String::from_utf8(result.stdout).unwrap();
    for field in [
        "accounts=100000",
        "depth=17",
        "account_seconds=",
        "tree_seconds=",
        "membership_seconds=",
        "image_id=",
        "account=public",
        "account=private",
        "user_cycles=",
        "padded_cycles=",
        "segments=",
        "permit=true",
    ] {
        assert!(output.contains(field), "missing metric: {field}");
    }
    for forbidden in [
        "390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c",
        "ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8",
        "siblings",
        "blinder",
        "nullifier_key",
    ] {
        assert!(!output.contains(forbidden));
    }
}

#[test]
fn measurement_command_refuses_development_proving() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_eligibility-host"))
        .env("RISC0_DEV_MODE", "1")
        .args(["prove", "sha256", "public", "composite"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("development proving is disabled")
    );
    assert!(result.stdout.is_empty());
}

#[test]
fn normal_public_guest_rejects_a_fake_eligibility_receipt() {
    let input = public_fixture();
    let permit =
        eligibility_core::check_eligibility(&input.policy, &input.purchase, &input.witness)
            .unwrap();
    let journal: Vec<u8> = risc0_zkvm::serde::to_vec(&Some(permit))
        .unwrap()
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let claim =
        risc0_zkvm::ReceiptClaim::ok(eligibility_methods::ELIGIBILITY_GUEST_ID, journal.clone());
    let receipt = risc0_zkvm::Receipt::new(
        risc0_zkvm::InnerReceipt::Fake(risc0_zkvm::FakeReceipt::new(claim)),
        journal,
    );
    let verification = eligibility_host::PublicVerification {
        receipt,
        policy: input.policy,
        purchase: input.purchase,
    };
    assert!(!eligibility_host::execute_public_verifier(&verification).unwrap());
}

#[test]
#[ignore = "opt-in real local proof for the 100,000-account list"]
fn real_private_receipt_binds_image_journal_and_purchase() {
    let prepared = benchmark::prepare(TreeHash::Sha256).unwrap();
    let input = prepared.private;
    let evidence =
        eligibility_host::prove(&input, eligibility_host::ReceiptMode::Composite).unwrap();
    let permit =
        eligibility_host::verify_receipt(&evidence.receipt, &input.policy, &input.purchase)
            .unwrap();
    assert_eq!(
        permit.nullifier,
        digest("216a87db55c348245382affbee432e0f1a34fa44daa40bac67fa754ca82b76d9")
    );
    assert!(evidence.receipt.verify([0_u32; 8]).is_err());
    let mut wrong_purchase = input.purchase;
    wrong_purchase.recipient[0] ^= 1;
    assert!(
        eligibility_host::verify_receipt(&evidence.receipt, &input.policy, &wrong_purchase)
            .is_err()
    );
    let mut altered = evidence.receipt.clone();
    altered.journal.bytes[4] ^= 1;
    assert!(eligibility_host::verify_receipt(&altered, &input.policy, &input.purchase).is_err());
    let public_check = eligibility_host::PublicVerification {
        receipt: evidence.receipt,
        policy: input.policy,
        purchase: input.purchase,
    };
    assert!(eligibility_host::verify_public_receipt(&public_check).is_err());
    assert!(!eligibility_host::execute_public_verifier(&public_check).unwrap());
}

#[test]
fn normal_public_guest_rejects_malformed_groth16_with_expected_public_statement() {
    use risc0_zkvm::sha::Digestible as _;
    let input = public_fixture();
    let permit =
        eligibility_core::check_eligibility(&input.policy, &input.purchase, &input.witness)
            .unwrap();
    let journal: Vec<u8> = risc0_zkvm::serde::to_vec(&Some(permit))
        .unwrap()
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    let claim =
        risc0_zkvm::ReceiptClaim::ok(eligibility_methods::ELIGIBILITY_GUEST_ID, journal.clone());
    for parameters in [
        risc0_zkvm::Groth16ReceiptVerifierParameters::default().digest(),
        risc0_zkvm::sha::Digest::ZERO,
    ] {
        let inner = risc0_zkvm::Groth16Receipt::new(
            vec![0; 256],
            risc0_zkvm::MaybePruned::Value(claim.clone()),
            parameters,
        );
        let receipt =
            risc0_zkvm::Receipt::new(risc0_zkvm::InnerReceipt::Groth16(inner), journal.clone());
        let verification = eligibility_host::PublicVerification {
            receipt,
            policy: input.policy.clone(),
            purchase: input.purchase,
        };
        assert!(eligibility_host::verify_public_receipt(&verification).is_err());
        assert!(!eligibility_host::execute_public_verifier(&verification).unwrap());
    }
}

#[test]
fn guest_authorizes_the_independently_bound_public_purchase() {
    let result = execute(&public_fixture()).unwrap();
    let permit = result.permit.unwrap();
    assert_eq!(
        permit.purchase,
        digest("3ef95bafc82fe12d758548697a12dfa79438939dc32ccadcaf44e0eea7278108")
    );
    assert_eq!(
        permit.nullifier,
        digest("f8bcfd4fa8fefb511c2708862bffc01f0f2267af81c272ef630becd72770db13")
    );
    assert!(result.user_cycles > 0);
}
