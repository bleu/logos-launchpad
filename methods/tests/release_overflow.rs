use risc0_binfmt::ProgramBinary;
use risc0_zkvm::{ExecutorEnv, default_executor};
use std::path::Path;

#[test]
fn host_release_arithmetic_rejects_overflow() {
    assert!(
        std::panic::catch_unwind(|| std::hint::black_box(u64::MAX) + 1).is_err(),
        "release arithmetic wrapped instead of rejecting overflow"
    );
}

#[test]
fn guest_release_arithmetic_rejects_overflow() {
    let methods = Path::new(env!("CARGO_MANIFEST_DIR"));
    let target = methods.join("target/overflow-probe");
    let mut command = risc0_build::cargo_command("build", &[]);
    if !command.get_args().any(|arg| arg == "--locked") {
        command.arg("--locked");
    }
    let status = command
        .args(["--release", "--example", "release_overflow"])
        .arg("--manifest-path")
        .arg(methods.join("guest/Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("could not start the guest compiler");
    assert!(status.success(), "release overflow guest did not build");

    let elf =
        std::fs::read(target.join("riscv32im-risc0-zkvm-elf/release/examples/release_overflow"))
            .expect("release overflow ELF is missing");
    let kernel = risc0_build::GuestOptions::default().kernel();
    let binary = ProgramBinary::new(&elf, &kernel).encode();
    let executor = default_executor();
    let control = ExecutorEnv::builder()
        .write(&(u64::MAX - 1, 1_u64))
        .unwrap()
        .build()
        .unwrap();
    let session = executor
        .execute(control, &binary)
        .expect("control guest failed");
    assert_eq!(session.journal.decode::<u64>().unwrap(), u64::MAX);

    let overflow = ExecutorEnv::builder()
        .write(&(u64::MAX, 1_u64))
        .unwrap()
        .build()
        .unwrap();
    match executor.execute(overflow, &binary) {
        Ok(session) => panic!(
            "release guest wrapped instead of rejecting overflow: {}",
            session.journal.decode::<u64>().unwrap()
        ),
        Err(error) => assert!(
            format!("{error:#}").contains("attempt to add with overflow"),
            "guest failed for a reason other than overflow: {error:#}"
        ),
    }
}
