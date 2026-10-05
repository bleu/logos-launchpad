use eligibility_core::{EligibilityPermit, Purchase, SalePolicy, check_public_binding};
use risc0_zkvm::{
    Groth16ReceiptVerifierParameters, InnerReceipt, Receipt, SuccinctReceiptVerifierParameters,
    VerifierContext,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum PublicReceiptMode {
    Groth16,
    Succinct,
}

#[derive(Serialize, Deserialize)]
pub struct PublicVerification {
    pub mode: PublicReceiptMode,
    pub receipt: Receipt,
    pub policy: SalePolicy,
    pub purchase: Purchase,
}

pub fn verify(
    input: &PublicVerification,
    trusted_image: [u32; 8],
) -> Result<EligibilityPermit, &'static str> {
    let context = match (input.mode, &input.receipt.inner) {
        (PublicReceiptMode::Groth16, InnerReceipt::Groth16(_)) => VerifierContext::empty()
            .with_dev_mode(false)
            .with_groth16_verifier_parameters(Groth16ReceiptVerifierParameters::default()),
        (PublicReceiptMode::Succinct, InnerReceipt::Succinct(_)) => VerifierContext::empty()
            .with_dev_mode(false)
            .with_suites(VerifierContext::default_hash_suites())
            .with_succinct_verifier_parameters(SuccinctReceiptVerifierParameters::default()),
        _ => return Err("unexpected receipt kind"),
    };
    input
        .receipt
        .verify_with_context(&context, trusted_image)
        .map_err(|_| "invalid receipt")?;
    let permit = input
        .receipt
        .journal
        .decode::<Option<EligibilityPermit>>()
        .map_err(|_| "invalid journal")?
        .ok_or("no permit")?;
    check_public_binding(&input.policy, &input.purchase, &permit)
        .map_err(|_| "purchase mismatch")?;
    let canonical: Vec<u8> = risc0_zkvm::serde::to_vec(&Some(&permit))
        .map_err(|_| "invalid journal")?
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    if canonical != input.receipt.journal.bytes {
        return Err("noncanonical journal");
    }
    Ok(permit)
}
