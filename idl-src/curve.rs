//! SPEL interface declaration for the hand-written curve guest.
//!
//! This source is intentionally declarative: `methods/guest/src/bin/curve.rs`
//! owns execution and `curve-core::Instruction` owns the wire enum.  SPEL scans
//! this file to produce the public interface without changing either boundary.

#[lez_program(instruction = "curve_core::Instruction")]
pub mod curve {
    #[instruction]
    pub fn renounce_admin(
        #[account(mut)] config: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        namespace: AccountId,
    ) {}

    #[instruction]
    pub fn create_pool(
        #[account(init)] pool: AccountWithMetadata,
        #[account(signer)] owner_authority: AccountWithMetadata,
        token0_definition: AccountWithMetadata,
        token1_definition: AccountWithMetadata,
        #[account(mut)] owner_token0_ata: AccountWithMetadata,
        #[account(mut)] owner_token1_ata: AccountWithMetadata,
        #[account(init)] pool_token0_ata: AccountWithMetadata,
        #[account(init)] pool_token1_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        allowed_input_token: Option<u8>,
        defer_funding: bool,
        namespace: AccountId,
        token0_amount: u128,
        token1_amount: u128,
        virtual_reserve0: u128,
        virtual_reserve1: u128,
        close_timestamp: Option<u64>,
        close_on_depletion: Option<u8>,
        owner: AccountId,
        owner_program_id: Option<ProgramId>,
        owner_program_seed: Option<[u8; 32]>,
        curve_program_id: ProgramId,
    ) {}

    #[instruction]
    pub fn swap_exact_input(
        #[account(mut)] pool: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(signer)] participant: AccountWithMetadata,
        #[account(mut)] participant_token_in_ata: AccountWithMetadata,
        #[account(mut)] pool_token_in_ata: AccountWithMetadata,
        #[account(mut)] pool_token_out_ata: AccountWithMetadata,
        #[account(mut)] participant_token_out_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        amount_in: u128,
        min_amount_out: u128,
        token_in: AccountId,
    ) {}

    #[instruction]
    pub fn swap_exact_output(
        #[account(mut)] pool: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(signer)] participant: AccountWithMetadata,
        #[account(mut)] participant_token_in_ata: AccountWithMetadata,
        #[account(mut)] pool_token_in_ata: AccountWithMetadata,
        #[account(mut)] pool_token_out_ata: AccountWithMetadata,
        #[account(mut)] participant_token_out_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        amount_out: u128,
        max_amount_in: u128,
        token_in: AccountId,
    ) {}

    #[instruction]
    pub fn close_pool(
        #[account(mut)] pool: AccountWithMetadata,
        #[account(signer)] owner: AccountWithMetadata,
        clock: AccountWithMetadata,
    ) {}

    #[instruction]
    pub fn activate_pool(
        #[account(mut)] pool: AccountWithMetadata,
        #[account(signer)] owner_authority: AccountWithMetadata,
        token0_definition: AccountWithMetadata,
        token1_definition: AccountWithMetadata,
        #[account(mut)] owner_token0_ata: AccountWithMetadata,
        #[account(mut)] owner_token1_ata: AccountWithMetadata,
        #[account(mut)] pool_token0_ata: AccountWithMetadata,
        #[account(mut)] pool_token1_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
    ) {}

    #[instruction]
    /// Omit the trailing treasury ATA when it matches an earlier account.
    pub fn withdraw_reserves(
        #[account(mut)] pool: AccountWithMetadata,
        #[account(signer)] owner: AccountWithMetadata,
        #[account(mut)] owner_token0_ata: AccountWithMetadata,
        #[account(mut)] owner_token1_ata: AccountWithMetadata,
        #[account(mut)] pool_token0_ata: AccountWithMetadata,
        #[account(mut)] pool_token1_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(mut)] treasury_collateral_atas: Vec<AccountWithMetadata>,
    ) {}
    #[instruction]
    /// Omit the trailing treasury ATA when it is the pool collateral ATA.
    pub fn collect_fees(
        #[account(mut)] pool: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        #[account(mut)] treasury_collateral_atas: Vec<AccountWithMetadata>,
    ) {}

    #[instruction]
    pub fn set_frozen(
        #[account(mut)] config: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        namespace: AccountId,
        frozen: bool,
    ) {}

    #[instruction]
    pub fn transfer_emergency_authority(
        #[account(mut)] config: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        namespace: AccountId,
        new_authority: AccountId,
    ) {}

    #[instruction]
    pub fn initialize_namespace(
        #[account(init)] config: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        namespace: AccountId,
        protocol_fee_bps: u32,
        treasury: AccountId,
    ) {}

    #[instruction]
    pub fn update_fee(
        #[account(mut)] config: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        namespace: AccountId,
        protocol_fee_bps: u32,
    ) {}

    #[instruction]
    pub fn update_treasury(
        #[account(mut)] config: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        namespace: AccountId,
        treasury: AccountId,
    ) {}

    #[instruction]
    /// None requires the successor's signature; Some contains eight program words and 32 seed bytes as words.
    pub fn transfer_admin(
        #[account(mut)] config: AccountWithMetadata,
        #[account(signer)] authority: AccountWithMetadata,
        successor: AccountWithMetadata,
        namespace: AccountId,
        new_admin: AccountId,
        candidate: Option<[u32; 40]>,
    ) {}
}
