use eligibility_core::{EligibilityInput, EligibilityPermit};
use eligibility_methods::ELIGIBILITY_GUEST_ELF;
use eyre::Result;
use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
use std::os::unix::fs::PermissionsExt;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtifactEvidence {
    pub directory_mode: u32,
    pub files: usize,
    pub bytes: u64,
    pub removed: bool,
}

fn protected_segments() -> Result<tempfile::TempDir> {
    let directory = tempfile::Builder::new()
        .prefix("eligibility-segments-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()?;
    if directory.path().metadata()?.permissions().mode() & 0o777 != 0o700 {
        eyre::bail!("private segment directory permissions are unsafe");
    }
    Ok(directory)
}

fn finish_segments(directory: tempfile::TempDir) -> Result<ArtifactEvidence> {
    let path = directory.path().to_owned();
    let directory_mode = path.metadata()?.permissions().mode() & 0o777;
    let mut files = 0;
    let mut bytes = 0;
    for entry in std::fs::read_dir(&path)? {
        let metadata = entry?.metadata()?;
        if metadata.is_file() {
            files += 1;
            bytes += metadata.len();
        }
    }
    directory.close()?;
    let removed = !path.try_exists()?;
    if !removed {
        eyre::bail!("private segment cleanup failed");
    }
    Ok(ArtifactEvidence {
        directory_mode,
        files,
        bytes,
        removed,
    })
}

pub mod native;
pub mod native_execution;
mod public_verification;
#[path = "../../methods/guest/src/trusted_image.rs"]
mod trusted_image;
pub use public_verification::{PublicReceiptMode, PublicVerification};

pub fn check_local_configuration() -> Result<()> {
    if [
        "RISC0_WITGEN_DEBUG",
        "RISC0_DUMP_PATH",
        "RISC0_PPROF_OUT",
        "RUST_LOG",
    ]
    .iter()
    .any(|name| std::env::var_os(name).is_some())
    {
        eyre::bail!("unsafe local proving configuration");
    }
    Ok(())
}

pub struct PublicExecution {
    pub accepted: bool,
    pub user_cycles: u64,
    pub input_bytes: usize,
    pub execute_seconds: f64,
}

pub fn execute_public_verifier(input: &PublicVerification) -> Result<bool> {
    Ok(measure_public_verifier(input)?.accepted)
}

pub fn measure_public_verifier(input: &PublicVerification) -> Result<PublicExecution> {
    use std::time::Instant;
    check_local_configuration()?;
    if trusted_image::EXPECTED_ELIGIBILITY_ID != eligibility_methods::ELIGIBILITY_GUEST_ID {
        eyre::bail!("public verifier image pin is stale");
    }
    let segments = protected_segments()?;
    let words = risc0_zkvm::serde::to_vec(input)?;
    let env = ExecutorEnv::builder()
        .segment_path(segments.path())
        .write_slice(&words)
        .session_limit(Some(33_554_432))
        .segment_limit_po2(20)
        .build()
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let server = std::env::var_os("RISC0_SERVER_PATH").unwrap_or_else(|| "r0vm".into());
    let started = Instant::now();
    let info = ExternalProver::new("local-ipc", server)
        .execute(env, eligibility_methods::PUBLIC_VERIFIER_ELF)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let execute_seconds = started.elapsed().as_secs_f64();
    finish_segments(segments)?;
    Ok(PublicExecution {
        accepted: info.journal.decode()?,
        user_cycles: info.cycles(),
        input_bytes: words.len() * 4,
        execute_seconds,
    })
}

pub fn verify_public_receipt(input: &PublicVerification) -> Result<EligibilityPermit> {
    public_verification::verify(input, eligibility_methods::ELIGIBILITY_GUEST_ID)
        .map_err(|message| eyre::eyre!(message))
}

pub struct Execution {
    pub permit: Option<EligibilityPermit>,
    pub artifacts: ArtifactEvidence,
    pub user_cycles: u64,
    pub padded_cycles: u64,
    pub segments: usize,
}

#[derive(Clone, Copy)]
pub enum ReceiptMode {
    Composite,
    Succinct,
}

pub struct ProofEvidence {
    pub receipt: risc0_zkvm::Receipt,
    pub artifacts: ArtifactEvidence,
    pub prove_seconds: f64,
    pub verify_seconds: f64,
    pub user_cycles: u64,
    pub total_cycles: u64,
    pub segments: usize,
}

pub fn prove(input: &EligibilityInput, mode: ReceiptMode) -> Result<ProofEvidence> {
    check_local_configuration()?;
    use risc0_zkvm::{Prover as _, ProverOpts, VerifierContext};
    use std::time::Instant;
    if std::env::var_os("RISC0_DEV_MODE").is_some_and(|value| value != "0") {
        eyre::bail!("RISC0_DEV_MODE must be unset or 0 for a real proof");
    }
    let expected =
        eligibility_core::check_eligibility(&input.policy, &input.purchase, &input.witness)
            .map_err(|_| eyre::eyre!("invalid eligibility input"))?;
    let segments = protected_segments()?;
    let env = ExecutorEnv::builder()
        .segment_path(segments.path())
        .write(input)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?
        .session_limit(Some(256_000_000))
        .segment_limit_po2(20)
        .build()
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let server = std::env::var_os("RISC0_SERVER_PATH").unwrap_or_else(|| "r0vm".into());
    let context = VerifierContext::default().with_dev_mode(false);
    let options = match mode {
        ReceiptMode::Composite => ProverOpts::composite(),
        ReceiptMode::Succinct => ProverOpts::succinct(),
    }
    .with_dev_mode(false);
    let started = Instant::now();
    let info = ExternalProver::new("local-ipc", server)
        .prove_with_ctx(env, &context, ELIGIBILITY_GUEST_ELF, &options)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let prove_seconds = started.elapsed().as_secs_f64();
    let started = Instant::now();
    let permit = verify_receipt(&info.receipt, &input.policy, &input.purchase)?;
    if permit != expected {
        eyre::bail!("verified journal differs from host checker");
    }
    let verify_seconds = started.elapsed().as_secs_f64();
    Ok(ProofEvidence {
        receipt: info.receipt,
        artifacts: finish_segments(segments)?,
        prove_seconds,
        verify_seconds,
        user_cycles: info.stats.user_cycles,
        total_cycles: info.stats.total_cycles,
        segments: info.stats.segments,
    })
}

pub fn verify_receipt(
    receipt: &risc0_zkvm::Receipt,
    policy: &eligibility_core::SalePolicy,
    purchase: &eligibility_core::Purchase,
) -> Result<EligibilityPermit> {
    let context = risc0_zkvm::VerifierContext::default().with_dev_mode(false);
    receipt.verify_with_context(&context, eligibility_methods::ELIGIBILITY_GUEST_ID)?;
    let permit = receipt
        .journal
        .decode::<Option<EligibilityPermit>>()?
        .ok_or_else(|| eyre::eyre!("no permit"))?;
    eligibility_core::check_public_binding(policy, purchase, &permit)
        .map_err(|_| eyre::eyre!("purchase binding mismatch"))?;
    let canonical: Vec<u8> = risc0_zkvm::serde::to_vec(&Some(&permit))?
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    if canonical != receipt.journal.bytes {
        eyre::bail!("noncanonical journal");
    }
    Ok(permit)
}

pub fn execute(input: &EligibilityInput) -> Result<Execution> {
    check_local_configuration()?;
    let segments = protected_segments()?;
    let env = ExecutorEnv::builder()
        .segment_path(segments.path())
        .write(input)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?
        .session_limit(Some(256_000_000))
        .segment_limit_po2(20)
        .build()
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let server = std::env::var_os("RISC0_SERVER_PATH").unwrap_or_else(|| "r0vm".into());
    let info = ExternalProver::new("local-ipc", server)
        .execute(env, ELIGIBILITY_GUEST_ELF)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    Ok(Execution {
        permit: info.journal.decode()?,
        artifacts: finish_segments(segments)?,
        user_cycles: info.cycles(),
        padded_cycles: info
            .segments
            .iter()
            .map(|segment| 1_u64 << segment.po2)
            .sum(),
        segments: info.segments.len(),
    })
}
