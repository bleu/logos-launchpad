//! SPEL interface declaration for the hand-written factory guest.

#[lez_program(instruction = "factory_core::Instruction")]
pub mod factory {
    #[instruction]
    pub fn create_factory_pool(
        #[account(init)] factory: AccountWithMetadata,
        #[account(init)] token_definition: AccountWithMetadata,
        #[account(init)] mint: AccountWithMetadata,
        #[account(init)] metadata: AccountWithMetadata,
        #[account(init)] creator_escrow: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        #[account(init)] creator_token_ata: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(init)] factory_token_ata: AccountWithMetadata,
        #[account(init)] factory_collateral_ata: AccountWithMetadata,
        #[account(init)] pool: AccountWithMetadata,
        #[account(init)] pool_token_ata: AccountWithMetadata,
        #[account(init)] pool_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        trade_mode: u8,
        graduation_hook_program_id: Option<ProgramId>,
        graduation_hook_configuration: Vec<u8>,
        namespace: AccountId,
        launch_salt: [u8; 32],
        name: String,
        uri: String,
        sale_reserve: u128,
        dex_seed_reserve: u128,
        creator_allocation: u128,
        virtual_token_reserve: u128,
        virtual_collateral_reserve: u128,
        end_timestamp: Option<u64>,
        curve_program_id: ProgramId,
    ) {
    }

    #[instruction]
    pub fn continue_creation(
        #[account(mut)] factory: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        #[account(mut)] mint: AccountWithMetadata,
        metadata: AccountWithMetadata,
        #[account(mut)] creator_escrow: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        #[account(mut)] creator_token_ata: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(mut)] factory_token_ata: AccountWithMetadata,
        #[account(mut)] factory_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
    ) {
    }

    #[instruction]
    pub fn claim_creator_allocation(
        #[account(mut)] factory: AccountWithMetadata,
        pool: AccountWithMetadata,
        #[account(mut)] creator_escrow: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        #[account(mut)] creator_token_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
    ) {
    }

    #[instruction]
    /// Omit trailing treasury_collateral_ata when already present in this account list.
    pub fn withdraw_factory_proceeds(
        #[account(mut)] factory: AccountWithMetadata,
        #[account(mut)] pool: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(mut)] factory_token_ata: AccountWithMetadata,
        #[account(mut)] factory_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        #[account(mut)] creator_token_ata: AccountWithMetadata,
        #[account(mut)] creator_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(mut)] treasury_collateral_atas: Vec<AccountWithMetadata>,
    ) {
    }

    #[instruction]
    /// Tail: optional distinct treasury ATA, then the counted adapter accounts in order.
    pub fn execute_graduation_hook(
        #[account(mut)] factory: AccountWithMetadata,
        #[account(mut)] pool: AccountWithMetadata,
        #[account(mut)] adapter_custody: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(mut)] factory_token_ata: AccountWithMetadata,
        #[account(mut)] factory_collateral_ata: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        #[account(mut)] adapter_token_ata: AccountWithMetadata,
        #[account(mut)] adapter_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        #[account(mut)] treasury_and_adapter_accounts: Vec<AccountWithMetadata>,
        remaining_accounts: u32,
    ) {
    }

    #[instruction]
    /// Flat fields encode FixedPriceInputs. Start None means Now; release 0 means AfterSetup, 1 AfterEnd.
    pub fn create_fixed_price_lbp(
        #[account(init)] factory: AccountWithMetadata,
        #[account(init)] token_definition: AccountWithMetadata,
        #[account(init)] mint: AccountWithMetadata,
        #[account(init)] metadata: AccountWithMetadata,
        #[account(init)] creator_escrow: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        #[account(init)] creator_token_ata: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(init)] factory_token_ata: AccountWithMetadata,
        #[account(init)] pool: AccountWithMetadata,
        #[account(init)] pool_token_ata: AccountWithMetadata,
        #[account(init)] pool_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        namespace: AccountId,
        launch_salt: [u8; 32],
        name: String,
        uri: String,
        sale_inventory: u128,
        creator_allocation: u128,
        collateral_units: u128,
        token_units: u128,
        start: Option<u64>,
        end_timestamp: Option<u64>,
        allocation_release: u32,
        namespace_program_id: ProgramId,
        lbp_program_id: ProgramId,
    ) {
    }

    #[instruction]
    /// Stage ordinals: Minted 0, Allocated 1, PoolPrepared 2, Complete 3.
    pub fn continue_fixed_price_creation(
        #[account(mut)] factory: AccountWithMetadata,
        token_definition: AccountWithMetadata,
        #[account(mut)] mint: AccountWithMetadata,
        metadata: AccountWithMetadata,
        #[account(mut)] creator_escrow: AccountWithMetadata,
        #[account(signer)] creator: AccountWithMetadata,
        #[account(mut)] creator_token_ata: AccountWithMetadata,
        collateral_definition: AccountWithMetadata,
        #[account(mut)] factory_token_ata: AccountWithMetadata,
        #[account(mut)] pool: AccountWithMetadata,
        #[account(mut)] pool_token_ata: AccountWithMetadata,
        #[account(mut)] pool_collateral_ata: AccountWithMetadata,
        clock: AccountWithMetadata,
        config: AccountWithMetadata,
        expected_stage: u32,
    ) {
    }
}
