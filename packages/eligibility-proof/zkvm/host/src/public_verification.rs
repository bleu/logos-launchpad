use eligibility_core::{EligibilityPermit, Purchase, SalePolicy, check_public_binding};
use risc0_zkvm::{Groth16ReceiptVerifierParameters, InnerReceipt, Receipt, VerifierContext};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct PublicVerification {
    pub receipt: Receipt,
    pub policy: SalePolicy,
    pub purchase: Purchase,
}

pub fn verify(
    input: &PublicVerification,
    trusted_image: [u32; 8],
) -> Result<EligibilityPermit, &'static str> {
    if !matches!(input.receipt.inner, InnerReceipt::Groth16(_)) {
        return Err("Groth16 receipt required");
    }
    let context = VerifierContext::empty()
        .with_dev_mode(false)
        .with_groth16_verifier_parameters(Groth16ReceiptVerifierParameters::default());
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
