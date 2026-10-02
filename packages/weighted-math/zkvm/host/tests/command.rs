use std::process::Command;

#[test]
fn proof_rejects_development_mode_before_starting_a_prover() {
    for value in ["1", "true", "TRUE", "yes"] {
        let output = Command::new(env!("CARGO_BIN_EXE_zkvm-host"))
            .arg("--prove-buy")
            .env("RISC0_DEV_MODE", value)
            .env("RISC0_PROVER", "unsupported")
            .env("RISC0_SERVER_PATH", "/missing-r0vm")
            .output()
            .expect("run host command");

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("RISC0_DEV_MODE must be unset or 0"),
            "{value}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn proof_rejects_extra_arguments_before_starting_a_prover() {
    for argument in ["--unexpected", "--prove-buy"] {
        let output = Command::new(env!("CARGO_BIN_EXE_zkvm-host"))
            .args(["--prove-buy", argument])
            .env("RISC0_DEV_MODE", "1")
            .output()
            .expect("run host command");

        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("unexpected argument"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn unknown_argument_is_rejected_before_guest_execution() {
    let output = Command::new(env!("CARGO_BIN_EXE_zkvm-host"))
        .arg("--unknown")
        .env("RISC0_EXECUTOR", "unsupported")
        .output()
        .expect("run host command");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("unknown argument"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
