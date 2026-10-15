//! SPEL wire declaration for the fixed-price sale guest.

#[lez_program(instruction = "lbp_core::Instruction")]
pub mod lbp {
    #[instruction]
    /// Flat fields encode FixedPricePoolInputs; release 0 means AfterSetup, 1 AfterEnd.
    pub fn prepare_fixed_price(
        #[account(init)] pool: AccountWithMetadata,
        #[account(signer)] factory_owner: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        factory_token_ata: AccountWithMetadata,
        #[account(init)] pool_token_ata: AccountWithMetadata,
        #[account(init)] pool_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        namespace: AccountId,
        namespace_program_id: ProgramId,
        factory_program_id: ProgramId,
        factory_seed: [u8; 32],
        sale_inventory: u128,
        creator_allocation: u128,
        collateral_units: u128,
        token_units: u128,
        start_timestamp: u64,
        end_timestamp: Option<u64>,
        creator_allocation_release: u32,
    ) {}

    #[instruction]
    pub fn activate_fixed_price(
        #[account(mut)] pool: AccountWithMetadata,
        #[account(signer)] factory_owner: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(mut)] factory_token_ata: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        pool_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
    ) {}

    #[instruction]
    pub fn buy_exact_input(
        #[account(mut)] pool: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(signer)] participant: AccountWithMetadata,
        #[account(mut)] participant_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        #[account(mut)] participant_token_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        amount_in: u128,
        min_amount_out: u128,
    ) {}

    #[instruction]
    pub fn buy_exact_output(
        #[account(mut)] pool: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(signer)] participant: AccountWithMetadata,
        #[account(mut)] participant_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        #[account(mut)] participant_token_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        amount_out: u128,
        max_amount_in: u128,
    ) {}

    #[instruction]
    /// Omit the trailing treasury ATA when it is the pool collateral ATA.
    pub fn collect_fees(
        #[account(mut)] pool: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        #[account(mut)] treasury_collateral_atas: Vec<AccountWithMetadata>,
    ) {}
}
