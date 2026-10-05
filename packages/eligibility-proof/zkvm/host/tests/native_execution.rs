use eligibility_core::{
    AccountControl, EligibilityInput, EligibilityWitness, Membership, Purchase, SaleContext,
    SalePolicy, TreeHash,
};
use eligibility_host::native_execution::{Caller, Trial, execute};

fn digest(value: &str) -> [u8; 32] {
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
        witness: EligibilityWitness {
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

fn private_fixture() -> EligibilityInput {
    let mut input = public_fixture();
    input.policy.root = digest("d29cf8dc06819098a9703ea750c10f30a9fa19b5f0acdae279b99dd394b419fb");
    input.witness.control = AccountControl::Private {
        nullifier_key: digest("390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c"),
        identifier: 0,
    };
    input
}

#[path = "../src/benchmark.rs"]
mod benchmark;

#[test]
#[ignore = "opt-in SHA-256 depth-17 native guest execution; no proof"]
fn native_guests_execute_depth17_lists_for_both_account_types() {
    let prepared = benchmark::prepare(TreeHash::Sha256).unwrap();
    assert!(prepared.account_seconds >= 0.0);
    assert!(prepared.tree_seconds >= 0.0);
    assert!(prepared.membership_seconds >= 0.0);
    for (account, input) in [("private", prepared.private), ("public", prepared.public)] {
        assert_eq!(input.policy.depth, 17);
        let evidence = execute(&Trial::new(input)).unwrap();
        assert!(evidence.accepted);
        assert!(evidence.marker_used);
        assert_eq!(evidence.purchase_count, 1);
        assert_eq!(evidence.collateral_in, 10);
        assert_eq!(evidence.tokens_out, 20);
        assert_eq!(evidence.program_runs, 2);
        assert!(evidence.user_cycles < 33_554_432);
        println!(
            "native_execution_only=true account={account} accounts=100000 depth=17 accepted={} program_runs={} user_cycles={} execute_seconds={:.6} artifact_files={} artifact_bytes={} artifacts_removed={}",
            evidence.accepted,
            evidence.program_runs,
            evidence.user_cycles,
            evidence.execute_seconds,
            evidence.artifacts.files,
            evidence.artifacts.bytes,
            evidence.artifacts.removed
        );
    }
}

#[test]
fn private_approved_account_executes_the_same_public_mock_purchase() {
    let evidence = execute(&Trial::new(private_fixture())).unwrap();
    assert!(evidence.accepted);
    assert!(evidence.marker_used);
    assert_eq!(evidence.purchase_count, 1);
    assert_eq!(evidence.collateral_in, 10);
    assert_eq!(evidence.tokens_out, 20);
    assert_eq!(evidence.program_runs, 2);
    assert!(evidence.user_cycles < 33_554_432);
}

#[test]
fn receiver_rejects_config_owned_by_a_foreign_program() {
    let mut trial = Trial::new(public_fixture());
    trial.caller = Caller::Foreign;
    trial.trusted_eligibility_id = [0xf0; 8];
    trial.config_owner = [0xf0; 8];
    let evidence = execute(&trial).unwrap();
    assert!(!evidence.accepted);
    assert_eq!(evidence.purchase_count, 0);
}

#[test]
fn eligibility_guest_rejects_over_cap_input_before_mock_purchase() {
    let mut trial = Trial::new(public_fixture());
    trial.input.purchase.collateral_in = 101;
    let evidence = execute(&trial).unwrap();
    assert!(!evidence.accepted);
    assert!(!evidence.marker_used);
    assert_eq!(evidence.purchase_count, 0);
    assert_eq!(evidence.program_runs, 1);
}

#[test]
fn receiver_uses_the_trusted_sale_root_and_common_cap() {
    for field in 0..3 {
        let mut trial = Trial::new(public_fixture());
        match field {
            0 => trial.trusted_policy.root[0] ^= 1,
            1 => trial.trusted_policy.collateral_limit = 101,
            2 => trial.trusted_policy.sale.sale_account[0] ^= 1,
            _ => unreachable!(),
        }
        let evidence = execute(&trial).unwrap();
        assert!(!evidence.accepted);
        assert!(!evidence.marker_used);
        assert_eq!(evidence.purchase_count, 0);
    }
}

#[test]
fn unavailable_executor_is_an_opaque_error_not_guest_rejection() {
    const PROBE: &str = "NATIVE_MISSING_EXECUTOR_PROBE";
    if std::env::var_os(PROBE).is_some() {
        let result = execute(&Trial::new(public_fixture()));
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().to_string(),
            "native execution failed; witness details are not logged"
        );
        return;
    }
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "unavailable_executor_is_an_opaque_error_not_guest_rejection",
        ])
        .env(PROBE, "1")
        .env("RISC0_SERVER_PATH", "/nonexistent-native-executor")
        .output()
        .unwrap();
    assert!(result.status.success());
}

#[test]
fn failed_mock_purchase_returns_no_accepted_execution_effects() {
    let mut trial = Trial::new(public_fixture());
    trial.fail_purchase = true;
    let evidence = execute(&trial).unwrap();
    assert!(!evidence.accepted);
    assert!(!evidence.marker_used);
    assert_eq!(evidence.purchase_count, 0);
    assert_eq!(evidence.program_runs, 2);
}

#[test]
fn duplicate_credential_cannot_reuse_a_spent_marker() {
    let mut trial = Trial::new(public_fixture());
    trial.marker_account.program_owner = trial.trusted_eligibility_id;
    trial.marker_account.data = vec![1].try_into().unwrap();
    let evidence = execute(&trial).unwrap();
    assert!(!evidence.accepted);
    assert!(evidence.marker_used);
    assert_eq!(evidence.purchase_count, 0);
}

#[test]
fn mock_purchase_rejects_an_independent_purchase_change() {
    let mut trial = Trial::new(public_fixture());
    trial.receiver_purchase.tokens_out = 21;
    let evidence = execute(&trial).unwrap();
    assert!(!evidence.accepted);
    assert!(!evidence.marker_used);
    assert_eq!(evidence.purchase_count, 0);
}

#[test]
fn receiver_rejects_a_forged_trusted_config_identity() {
    let mut trial = Trial::new(public_fixture());
    trial.config_account_id = lee_core::account::AccountId::new([0xc2; 32]);
    let evidence = execute(&trial).unwrap();
    assert!(!evidence.accepted);
    assert!(!evidence.marker_used);
    assert_eq!(evidence.purchase_count, 0);
}

#[test]
fn direct_and_foreign_callers_cannot_execute_mock_purchase() {
    for caller in [Caller::Direct, Caller::Foreign] {
        let mut trial = Trial::new(public_fixture());
        trial.caller = caller;
        let evidence = execute(&trial).unwrap();
        assert!(!evidence.accepted);
        assert!(!evidence.marker_used);
        assert_eq!(evidence.purchase_count, 0);
    }
}

#[test]
fn trusted_chained_caller_executes_marker_and_mock_purchase() {
    let evidence = execute(&Trial::new(public_fixture())).unwrap();
    assert!(evidence.accepted);
    assert!(evidence.marker_used);
    assert_eq!(evidence.purchase_count, 1);
    assert_eq!(evidence.collateral_in, 10);
    assert_eq!(evidence.tokens_out, 20);
    assert_eq!(evidence.program_runs, 2);
    assert!(evidence.user_cycles > 0);
    assert!(evidence.artifacts.removed);
}
