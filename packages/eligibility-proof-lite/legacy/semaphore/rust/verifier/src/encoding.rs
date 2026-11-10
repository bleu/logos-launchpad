use ark_bn254::{Fq, Fq2, G1Affine, G2Affine};
use ark_ff::PrimeField;

#[derive(Debug, PartialEq, Eq)]
pub enum VerifyError {
    MalformedJson,
    InvalidEncoding,
    InvalidPoint,
    InvalidProof,
    InvalidVerificationKey,
    PublicInputsMismatch,
}

pub(crate) fn g1(value: &[String]) -> Result<G1Affine, VerifyError> {
    if !matches!(value.len(), 2 | 3) || (value.len() == 3 && value[2] != "1") {
        return Err(VerifyError::InvalidEncoding);
    }
    let point = G1Affine::new_unchecked(decimal::<Fq>(&value[0])?, decimal::<Fq>(&value[1])?);
    if !point.is_on_curve() || !point.is_in_correct_subgroup_assuming_on_curve() {
        return Err(VerifyError::InvalidPoint);
    }
    Ok(point)
}

pub(crate) fn g2(value: &[[String; 2]]) -> Result<G2Affine, VerifyError> {
    if !matches!(value.len(), 2 | 3) || (value.len() == 3 && value[2] != ["1", "0"]) {
        return Err(VerifyError::InvalidEncoding);
    }
    let coordinate = |pair: &[String; 2]| -> Result<Fq2, VerifyError> {
        Ok(Fq2::new(decimal::<Fq>(&pair[0])?, decimal::<Fq>(&pair[1])?))
    };
    let point = G2Affine::new_unchecked(coordinate(&value[0])?, coordinate(&value[1])?);
    if !point.is_on_curve() || !point.is_in_correct_subgroup_assuming_on_curve() {
        return Err(VerifyError::InvalidPoint);
    }
    Ok(point)
}

pub(crate) fn decimal<F: PrimeField>(value: &str) -> Result<F, VerifyError> {
    let modulus = F::MODULUS.to_string();
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
        || value.len() > modulus.len()
        || (value.len() == modulus.len() && value >= modulus.as_str())
    {
        return Err(VerifyError::InvalidEncoding);
    }
    value.parse().map_err(|_| VerifyError::InvalidEncoding)
}
