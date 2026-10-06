use borsh::{BorshDeserialize, BorshSerialize};

pub mod compatibility;

#[derive(Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SaleOperation {
    pub version: u8,
    pub namespace: [u8; 32],
    pub sale: [u8; 32],
    pub operation: u8,
}

#[derive(BorshSerialize, BorshDeserialize)]
pub enum ProbeInstruction {
    Emit(SaleOperation),
    EmitThenFail(SaleOperation),
    Fail,
}

pub fn decode(data: &[u8]) -> Result<SaleOperation, std::io::Error> {
    let event: SaleOperation = borsh::from_slice(data)?;
    if event.version != 1 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "unsupported sale operation schema",
        ));
    }
    Ok(event)
}
