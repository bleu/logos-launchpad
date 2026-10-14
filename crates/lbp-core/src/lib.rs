//! Fixed-price sale setup and funding, separate from curve pricing and lifecycle.

use associated_token_account_core::Instruction as AtaInstruction;
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{AccountPostState, ChainedCall, Claim, PdaSeed, ProgramId},
};
use serde::{Deserialize, Serialize};
use token_core::{TokenDefinition, TokenHolding};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub struct FixedPrice {
    pub collateral_units: u128,
    pub token_units: u128,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum CreatorAllocationRelease {
    AfterSetup,
    AfterEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct FixedPricePoolInputs {
    pub namespace: AccountId,
    pub namespace_program_id: ProgramId,
    pub factory_program_id: ProgramId,
    pub factory_seed: [u8; 32],
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub price: FixedPrice,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub creator_allocation_release: CreatorAllocationRelease,
}

/// Account order: pool, factory owner, token/collateral definitions, factory token ATA, pool token/collateral ATAs, clock, namespace config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::large_enum_variant,
    reason = "the public instruction keeps typed pool inputs"
)]
pub enum Instruction {
    PrepareFixedPrice { inputs: FixedPricePoolInputs },
    ActivateFixedPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PoolMode {
    FixedPrice,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PoolAccount {
    pub mode: PoolMode,
    pub funded: bool,
    pub namespace: AccountId,
    pub namespace_program_id: ProgramId,
    pub factory_owner: AccountId,
    pub factory_program_id: ProgramId,
    pub factory_seed: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub price: FixedPrice,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub creator_allocation_release: CreatorAllocationRelease,
    pub recorded_collateral: u128,
}

impl TryFrom<&Data> for PoolAccount {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        Self::try_from_slice(data.as_ref())
    }
}

impl From<&PoolAccount> for Data {
    fn from(state: &PoolAccount) -> Self {
        Self::try_from(borsh::to_vec(state).expect("Pool serialization must succeed"))
            .expect("Pool state must fit in account data")
    }
}

#[must_use]
pub fn compute_pool_pda(
    namespace: AccountId,
    lbp_program_id: ProgramId,
    token_definition_id: AccountId,
    collateral_definition_id: AccountId,
    factory_owner: AccountId,
) -> AccountId {
    curve_core::compute_pool_pda(
        namespace,
        lbp_program_id,
        token_definition_id,
        collateral_definition_id,
        factory_owner,
    )
}

#[must_use]
pub fn compute_pool_pda_seed(
    namespace: AccountId,
    token_definition_id: AccountId,
    collateral_definition_id: AccountId,
    factory_owner: AccountId,
) -> PdaSeed {
    curve_core::compute_pool_pda_seed(
        namespace,
        token_definition_id,
        collateral_definition_id,
        factory_owner,
    )
}

#[must_use]
pub fn process_instruction(
    pre: Vec<AccountWithMetadata>,
    instruction: Instruction,
    lbp_program_id: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    match instruction {
        Instruction::PrepareFixedPrice { inputs } => prepare(pre, inputs, lbp_program_id),
        Instruction::ActivateFixedPrice => activate(pre, lbp_program_id),
    }
}

fn holding_balance(holding: &AccountWithMetadata, definition: AccountId) -> u128 {
    assert_eq!(
        holding.account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Token holding must be owned by the pinned token program"
    );
    let TokenHolding::Fungible {
        definition_id,
        balance,
    } = TokenHolding::try_from(&holding.account.data).expect("Token holding data is invalid")
    else {
        panic!("Token holding must be fungible")
    };
    assert_eq!(
        definition_id, definition,
        "Token holding has the wrong definition"
    );
    balance
}

fn validate_custody_addresses(
    pool: &AccountWithMetadata,
    factory: &AccountWithMetadata,
    token: AccountId,
    collateral: AccountId,
    source: &AccountWithMetadata,
    token_vault: &AccountWithMetadata,
    collateral_vault: &AccountWithMetadata,
) {
    for (holding, owner, definition) in [
        (source, factory, token),
        (token_vault, pool, token),
        (collateral_vault, pool, collateral),
    ] {
        associated_token_account_core::verify_ata_and_get_seed(
            holding,
            owner,
            definition,
            curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        );
    }
}

fn validate_definition(definition: &AccountWithMetadata) {
    assert_eq!(
        definition.account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Token definition must be owned by the pinned token program"
    );
    assert!(
        matches!(
            TokenDefinition::try_from(&definition.account.data)
                .expect("Token definition data is invalid"),
            TokenDefinition::Fungible { .. }
        ),
        "Token definition must be fungible"
    );
}

fn trusted_time(clock: &AccountWithMetadata) -> u64 {
    assert_eq!(
        clock.account_id,
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        "Trusted LEZ clock is required"
    );
    clock_core::ClockAccountData::from_bytes(clock.account.data.as_ref()).timestamp
}

fn validate_factory(
    owner: &AccountWithMetadata,
    factory_program_id: ProgramId,
    factory_seed: [u8; 32],
) {
    assert!(
        owner.is_authorized,
        "Factory authority authorization is missing"
    );
    assert_eq!(
        owner.account.program_owner, factory_program_id,
        "Factory authority has the wrong program owner"
    );
    assert_eq!(
        owner.account_id,
        AccountId::for_public_pda(&factory_program_id, &PdaSeed::new(factory_seed)),
        "Factory authority must be its derived PDA"
    );
}

fn activate(
    pre: Vec<AccountWithMetadata>,
    lbp_program_id: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let [
        pool,
        factory_owner,
        token,
        collateral,
        source,
        vault,
        collateral_vault,
        clock,
        config,
    ]: [_; 9] = pre
        .clone()
        .try_into()
        .expect("ActivateFixedPrice requires nine accounts");
    let now = trusted_time(&clock);
    let mut state = PoolAccount::try_from(&pool.account.data).expect("Invalid prepared pool");
    assert!(!state.funded, "Pool is already funded");
    assert_eq!(
        state.recorded_collateral, 0,
        "Pending pool must have zero recorded collateral"
    );
    assert_eq!(
        pool.account.program_owner, lbp_program_id,
        "Pool program owner is invalid"
    );
    assert_eq!(
        pool.account_id,
        compute_pool_pda(
            state.namespace,
            lbp_program_id,
            state.token_definition_id,
            state.collateral_definition_id,
            state.factory_owner
        ),
        "Pool account ID does not match PDA"
    );
    assert!(now < state.end_timestamp, "Sale has ended");
    curve_core::validated_config(&config, state.namespace, state.namespace_program_id);
    validate_factory(&factory_owner, state.factory_program_id, state.factory_seed);
    assert_eq!(
        factory_owner.account_id, state.factory_owner,
        "Factory authority is not the saved owner"
    );
    validate_definition(&token);
    validate_definition(&collateral);
    assert_eq!(
        token.account_id, state.token_definition_id,
        "Definition ID does not match saved pool"
    );
    assert_eq!(
        collateral.account_id, state.collateral_definition_id,
        "Definition ID does not match saved pool"
    );
    validate_custody_addresses(
        &pool,
        &factory_owner,
        state.token_definition_id,
        state.collateral_definition_id,
        &source,
        &vault,
        &collateral_vault,
    );
    assert_eq!(
        holding_balance(&source, state.token_definition_id),
        state.sale_inventory,
        "Factory inventory must equal sale inventory"
    );
    assert_eq!(
        holding_balance(&vault, state.token_definition_id),
        0,
        "Pool vault must be empty"
    );
    holding_balance(&collateral_vault, state.collateral_definition_id);
    let call = ChainedCall::new(
        curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        vec![factory_owner, source, vault],
        &AtaInstruction::Transfer {
            ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            amount: state.sale_inventory,
        },
    );
    state.funded = true;
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|p| AccountPostState::new(p.account))
        .collect();
    posts[0].account_mut().data = Data::from(&state);
    (posts, vec![call])
}

fn prepare(
    pre: Vec<AccountWithMetadata>,
    inputs: FixedPricePoolInputs,
    lbp_program_id: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let [
        pool,
        factory_owner,
        token,
        collateral,
        source,
        token_vault,
        collateral_vault,
        clock,
        config,
    ]: [_; 9] = pre
        .clone()
        .try_into()
        .expect("PrepareFixedPrice requires nine accounts");
    assert_eq!(
        pool.account,
        Account::default(),
        "Pool is already initialized"
    );
    let now = trusted_time(&clock);
    validate_factory(
        &factory_owner,
        inputs.factory_program_id,
        inputs.factory_seed,
    );
    assert!(inputs.sale_inventory > 0, "Sale inventory must be positive");
    assert!(
        inputs.price.collateral_units > 0 && inputs.price.token_units > 0,
        "Price units must be positive"
    );
    assert!(
        inputs.end_timestamp > inputs.start_timestamp,
        "End must be after start"
    );
    assert!(now < inputs.end_timestamp, "Sale has ended");
    curve_core::validated_config(&config, inputs.namespace, inputs.namespace_program_id);
    assert_ne!(
        token.account_id, collateral.account_id,
        "Pool definitions must differ"
    );
    assert_eq!(
        pool.account_id,
        compute_pool_pda(
            inputs.namespace,
            lbp_program_id,
            token.account_id,
            collateral.account_id,
            factory_owner.account_id
        ),
        "Pool account ID does not match PDA"
    );
    validate_definition(&token);
    validate_definition(&collateral);
    validate_custody_addresses(
        &pool,
        &factory_owner,
        token.account_id,
        collateral.account_id,
        &source,
        &token_vault,
        &collateral_vault,
    );
    holding_balance(&source, token.account_id);
    for (vault, definition) in [
        (&token_vault, token.account_id),
        (&collateral_vault, collateral.account_id),
    ] {
        if vault.account != Account::default() {
            assert_eq!(
                holding_balance(vault, definition),
                0,
                "Pool vault must be empty"
            );
        }
    }
    let state = PoolAccount {
        mode: PoolMode::FixedPrice,
        funded: false,
        namespace: inputs.namespace,
        namespace_program_id: inputs.namespace_program_id,
        factory_owner: factory_owner.account_id,
        factory_program_id: inputs.factory_program_id,
        factory_seed: inputs.factory_seed,
        token_definition_id: token.account_id,
        collateral_definition_id: collateral.account_id,
        sale_inventory: inputs.sale_inventory,
        creator_allocation: inputs.creator_allocation,
        price: inputs.price,
        start_timestamp: inputs.start_timestamp,
        end_timestamp: inputs.end_timestamp,
        creator_allocation_release: inputs.creator_allocation_release,
        recorded_collateral: 0,
    };
    let mut pool_post = pool.account;
    pool_post.data = Data::from(&state);
    let pool_owner = AccountWithMetadata {
        account: Account {
            program_owner: lbp_program_id,
            ..pool_post.clone()
        },
        is_authorized: false,
        account_id: pool.account_id,
    };
    let calls = [(token, token_vault), (collateral, collateral_vault)]
        .into_iter()
        .map(|(definition, vault)| {
            ChainedCall::new(
                curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                vec![pool_owner.clone(), definition, vault],
                &AtaInstruction::Create {
                    ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                },
            )
        })
        .collect();
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|p| AccountPostState::new(p.account))
        .collect();
    posts[0] = AccountPostState::new_claimed_if_default(
        pool_post,
        Claim::Pda(compute_pool_pda_seed(
            state.namespace,
            state.token_definition_id,
            state.collateral_definition_id,
            state.factory_owner,
        )),
    );
    (posts, calls)
}
