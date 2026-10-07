use std::process::Command;

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
