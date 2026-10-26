#[derive(BorshSerialize, BorshDeserialize)]
pub enum Message {
    ReadState,
    Start { start_timestamp: u64 },
}

pub enum Error {
    InvalidState,
}
