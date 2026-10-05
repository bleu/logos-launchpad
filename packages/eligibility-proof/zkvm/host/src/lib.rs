use eligibility_core::{EligibilityInput, EligibilityPermit};
use eligibility_methods::ELIGIBILITY_GUEST_ELF;
use eyre::Result;
use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};

mod public_verification;
#[path = "../../methods/guest/src/trusted_image.rs"]
mod trusted_image;
pub use public_verification::PublicVerification;

pub fn execute_public_verifier(input: &PublicVerification) -> Result<bool> {
    if trusted_image::EXPECTED_ELIGIBILITY_ID != eligibility_methods::ELIGIBILITY_GUEST_ID {
        eyre::bail!("public verifier image pin is stale");
    }
    let env = ExecutorEnv::builder()
        .write(input)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?
        .session_limit(Some(256_000_000))
        .segment_limit_po2(20)
        .build()
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    let server = std::env::var_os("RISC0_SERVER_PATH").unwrap_or_else(|| "r0vm".into());
    let info = ExternalProver::new("local-ipc", server)
        .execute(env, eligibility_methods::PUBLIC_VERIFIER_ELF)
        .map_err(|error| eyre::eyre!(error.into_boxed_dyn_error()))?;
    Ok(info.journal.decode()?)
}

pub fn verify_public_receipt(input: &PublicVerification) -> Result<EligibilityPermit> {
    public_verification::verify(input, eligibility_methods::ELIGIBILITY_GUEST_ID)
        .map_err(|message| eyre::eyre!(message))
}

pub struct Execution {
    pub permit: Option<EligibilityPermit>,
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
    pub prove_seconds: f64,
    pub verify_seconds: f64,
    pub user_cycles: u64,
    pub total_cycles: u64,
    pub segments: usize,
}

pub fn prove(input: &EligibilityInput, mode: ReceiptMode) -> Result<ProofEvidence> {
    use risc0_zkvm::{Prover as _, ProverOpts, VerifierContext};
    use std::time::Instant;
    if std::env::var_os("RISC0_DEV_MODE").is_some_and(|value| value != "0") {
        eyre::bail!("RISC0_DEV_MODE must be unset or 0 for a real proof");
    }
    let expected =
        eligibility_core::check_eligibility(&input.policy, &input.purchase, &input.witness)
            .map_err(|_| eyre::eyre!("invalid eligibility input"))?;
    let env = ExecutorEnv::builder()
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
    Ok(ProofEvidence {
        receipt: info.receipt,
        prove_seconds,
        verify_seconds: started.elapsed().as_secs_f64(),
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
    let env = ExecutorEnv::builder()
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
        user_cycles: info.cycles(),
        padded_cycles: info
            .segments
            .iter()
            .map(|segment| 1_u64 << segment.po2)
            .sum(),
        segments: info.segments.len(),
    })
}
