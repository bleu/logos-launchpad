//! SPEL wire declaration for the fixed-price LBP guest.

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
        end_timestamp: u64,
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
}
