use admin_authority::{AdminCandidate, AdminConfig};
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{AccountPostState, ChainedCall, Claim, ProgramId},
};
use serde::{Deserialize, Serialize};
use spel_framework::prelude as spel;

pub use lbp_core::{CreatorAllocationRelease, FixedPrice};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SaleStart {
    Now,
    At { timestamp: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FixedPriceInputs {
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub name: String,
    pub uri: String,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub price: FixedPrice,
    pub start: SaleStart,
    pub end_timestamp: u64,
    pub allocation_release: CreatorAllocationRelease,
    pub namespace_program_id: ProgramId,
    pub lbp_program_id: ProgramId,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum FixedPriceCreationStage {
    Minted,
    Allocated,
    PoolPrepared,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct FixedPriceLaunchState {
    format: [u8; 8],
    version: u16,
    creator_authority: AdminConfig,
    pub mode: lbp_core::PoolMode,
    pub creation_stage: FixedPriceCreationStage,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub creator_escrow_id: AccountId,
    pub pool_id: AccountId,
    pub namespace_program_id: ProgramId,
    pub lbp_program_id: ProgramId,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub total_supply: u128,
    pub price: FixedPrice,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub allocation_release: CreatorAllocationRelease,
    pub creator_allocation_released: bool,
}

impl FixedPriceLaunchState {
    const CREATOR_OFFSET: usize = 10;

    pub fn assert_creator(&self, creator: &AccountWithMetadata) {
        self.creator_authority
            .assert_admin(&adapt(creator))
            .expect("Sale creator authorization failed");
    }
}

impl TryFrom<&Data> for FixedPriceLaunchState {
    type Error = std::io::Error;

    fn try_from(data: &Data) -> Result<Self, Self::Error> {
        let state = Self::try_from_slice(data.as_ref())?;
        if state.format != *b"LZFXLAUN" || state.version != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Unsupported fixed-price launch state",
            ));
        }
        Ok(state)
    }
}

impl From<&FixedPriceLaunchState> for Data {
    fn from(state: &FixedPriceLaunchState) -> Self {
        borsh::to_vec(state)
            .expect("Fixed-price launch serializes")
            .try_into()
            .expect("Fixed-price launch fits account data")
    }
}

// The authority library uses a different LEZ crate identity.
fn adapt(source: &AccountWithMetadata) -> spel::AccountWithMetadata {
    spel::AccountWithMetadata {
        account_id: spel::AccountId::new(source.account_id.to_bytes()),
        is_authorized: source.is_authorized,
        account: spel::Account {
            program_owner: source.account.program_owner,
            balance: source.account.balance,
            data: source
                .account
                .data
                .to_vec()
                .try_into()
                .expect("Authority account fits"),
            nonce: source.account.nonce.0.into(),
        },
    }
}

pub(crate) fn create(
    pre: Vec<AccountWithMetadata>,
    inputs: FixedPriceInputs,
    program: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let [
        factory,
        definition,
        mint,
        metadata,
        escrow,
        creator,
        _creator_ata,
        collateral,
        _factory_ata,
        pool,
        _pool_token,
        _pool_collateral,
        clock,
        _config,
    ]: [_; 14] = pre
        .clone()
        .try_into()
        .expect("CreateFixedPriceLbp requires fourteen accounts");
    assert_ne!(
        inputs.namespace,
        AccountId::default(),
        "Namespace must not be empty"
    );
    assert_eq!(
        factory.account_id,
        super::compute_factory_pda(inputs.namespace, program, inputs.launch_salt),
        "Factory account ID does not match PDA"
    );
    assert_eq!(
        definition.account_id,
        super::compute_definition_pda(inputs.namespace, program, inputs.launch_salt),
        "Wrong launch definition PDA"
    );
    assert_eq!(
        mint.account_id,
        super::compute_mint_pda(inputs.namespace, program, inputs.launch_salt),
        "Wrong mint PDA"
    );
    assert_eq!(
        metadata.account_id,
        super::compute_metadata_pda(inputs.namespace, program, inputs.launch_salt),
        "Wrong metadata PDA"
    );
    assert_eq!(
        escrow.account_id,
        super::compute_escrow_pda(inputs.namespace, program, inputs.launch_salt),
        "Wrong creator escrow PDA"
    );
    assert_eq!(
        pool.account_id,
        curve_core::compute_pool_pda(
            inputs.namespace,
            inputs.lbp_program_id,
            definition.account_id,
            collateral.account_id,
            factory.account_id
        ),
        "Wrong fixed-price pool PDA"
    );
    for (index, owner, token) in [
        (6, &creator, definition.account_id),
        (8, &factory, definition.account_id),
        (10, &pool, definition.account_id),
        (11, &pool, collateral.account_id),
    ] {
        associated_token_account_core::verify_ata_and_get_seed(
            &pre[index],
            owner,
            token,
            curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        );
    }
    assert_eq!(
        collateral.account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Collateral definition owner does not match token program"
    );
    assert!(
        matches!(
            token_core::TokenDefinition::try_from(&collateral.account.data)
                .expect("Invalid collateral definition"),
            token_core::TokenDefinition::Fungible { .. }
        ),
        "Collateral must be fungible"
    );
    curve_core::validated_config(&pre[13], inputs.namespace, inputs.namespace_program_id);
    for index in [1, 2, 3, 4, 8, 9, 10, 11] {
        assert_eq!(
            pre[index].account,
            Account::default(),
            "Setup account already exists"
        );
    }
    assert_eq!(factory.account, Account::default(), "Launch already exists");
    assert!(!inputs.name.is_empty(), "Token name must not be empty");
    assert!(
        !inputs.uri.is_empty(),
        "Token metadata URI must not be empty"
    );
    assert!(inputs.sale_inventory > 0, "Sale inventory must be positive");
    assert!(
        inputs.price.collateral_units > 0 && inputs.price.token_units > 0,
        "Fixed price ratio must be positive"
    );
    let now = super::trusted_time(&clock);
    let start_timestamp = match inputs.start {
        SaleStart::Now => now,
        SaleStart::At { timestamp } => {
            assert!(timestamp >= now, "Scheduled start is in the past");
            timestamp
        }
    };
    assert!(
        inputs.end_timestamp > start_timestamp,
        "Sale end must be after start"
    );
    let state = FixedPriceLaunchState {
        format: *b"LZFXLAUN",
        version: 1,
        creator_authority: AdminConfig::default(),
        mode: lbp_core::PoolMode::FixedPrice,
        creation_stage: FixedPriceCreationStage::Minted,
        namespace: inputs.namespace,
        launch_salt: inputs.launch_salt,
        token_definition_id: definition.account_id,
        collateral_definition_id: collateral.account_id,
        creator_escrow_id: escrow.account_id,
        pool_id: pool.account_id,
        namespace_program_id: inputs.namespace_program_id,
        lbp_program_id: inputs.lbp_program_id,
        sale_inventory: inputs.sale_inventory,
        creator_allocation: inputs.creator_allocation,
        total_supply: inputs
            .sale_inventory
            .checked_add(inputs.creator_allocation)
            .expect("Launch supply overflow"),
        price: inputs.price,
        start_timestamp,
        end_timestamp: inputs.end_timestamp,
        allocation_release: inputs.allocation_release,
        creator_allocation_released: false,
    };
    let mut embedded = adapt(&factory);
    embedded.account.data = Data::from(&state).to_vec().try_into().expect("Launch fits");
    AdminConfig::bootstrap_at(
        &mut embedded,
        FixedPriceLaunchState::CREATOR_OFFSET,
        AdminCandidate::Signer,
        &adapt(&creator),
    )
    .expect("Sale creator authorization failed");
    let mut post = factory.account.clone();
    post.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Launch fits");
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|a| AccountPostState::new(a.account))
        .collect();
    posts[0] = AccountPostState::new_claimed_if_default(
        post,
        Claim::Pda(super::compute_factory_seed(
            inputs.namespace,
            inputs.launch_salt,
        )),
    );
    let call = ChainedCall::new(
        curve_core::TOKEN_PROGRAM_ID,
        vec![
            AccountWithMetadata {
                is_authorized: true,
                ..definition
            },
            AccountWithMetadata {
                is_authorized: true,
                ..mint
            },
            AccountWithMetadata {
                is_authorized: true,
                ..metadata
            },
        ],
        &token_core::Instruction::NewDefinitionWithMetadata {
            new_definition: token_core::NewTokenDefinition::Fungible {
                name: inputs.name,
                total_supply: state.total_supply,
            },
            metadata: Box::new(token_core::NewTokenMetadata {
                standard: token_core::MetadataStandard::Simple,
                uri: inputs.uri,
                creators: String::new(),
            }),
        },
    )
    .with_pda_seeds(vec![
        super::compute_definition_seed(inputs.namespace, inputs.launch_salt),
        super::compute_mint_seed(inputs.namespace, inputs.launch_salt),
        super::compute_metadata_seed(inputs.namespace, inputs.launch_salt),
    ]);
    (posts, vec![call])
}

fn holding_balance(account: &AccountWithMetadata, definition: AccountId) -> u128 {
    assert_eq!(
        account.account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Wrong token holding owner"
    );
    match token_core::TokenHolding::try_from(&account.account.data).expect("Invalid token holding")
    {
        token_core::TokenHolding::Fungible {
            definition_id,
            balance,
        } => {
            assert_eq!(definition_id, definition, "Wrong token holding definition");
            balance
        }
        _ => panic!("Token holding must be fungible"),
    }
}

pub(crate) fn continue_creation(
    pre: Vec<AccountWithMetadata>,
    expected_stage: FixedPriceCreationStage,
    program: ProgramId,
) -> (Vec<AccountPostState>, Vec<ChainedCall>) {
    let [
        factory,
        definition,
        mint,
        _metadata,
        escrow,
        creator,
        _creator_ata,
        _collateral,
        factory_token,
        _pool,
        _pool_token,
        _pool_collateral,
        _clock,
        _config,
    ]: [_; 14] = pre
        .clone()
        .try_into()
        .expect("ContinueFixedPriceCreation requires fourteen accounts");
    let mut state =
        FixedPriceLaunchState::try_from(&factory.account.data).expect("Invalid launch state");
    state.assert_creator(&creator);
    assert_eq!(
        factory.account.program_owner, program,
        "Wrong factory program owner"
    );
    assert_eq!(
        factory.account_id,
        super::compute_factory_pda(state.namespace, program, state.launch_salt),
        "Wrong factory PDA"
    );
    assert_eq!(
        definition.account_id, state.token_definition_id,
        "Wrong launch definition"
    );
    assert_eq!(
        definition.account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Wrong launch definition owner"
    );
    match token_core::TokenDefinition::try_from(&definition.account.data)
        .expect("Invalid launch definition")
    {
        token_core::TokenDefinition::Fungible {
            total_supply,
            metadata_id,
            ..
        } => {
            assert_eq!(total_supply, state.total_supply, "Launch supply changed");
            assert_eq!(
                metadata_id,
                Some(pre[3].account_id),
                "Launch metadata changed"
            );
        }
        _ => panic!("Launch definition must be fungible"),
    }
    assert_eq!(
        mint.account_id,
        super::compute_mint_pda(state.namespace, program, state.launch_salt),
        "Wrong mint PDA"
    );
    assert_eq!(
        pre[3].account_id,
        super::compute_metadata_pda(state.namespace, program, state.launch_salt),
        "Wrong metadata PDA"
    );
    assert_eq!(
        escrow.account_id, state.creator_escrow_id,
        "Wrong creator escrow"
    );
    assert_eq!(
        pre[7].account_id, state.collateral_definition_id,
        "Wrong collateral definition"
    );
    assert_eq!(
        pre[7].account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Wrong collateral definition owner"
    );
    assert!(
        matches!(
            token_core::TokenDefinition::try_from(&pre[7].account.data)
                .expect("Invalid collateral definition"),
            token_core::TokenDefinition::Fungible { .. }
        ),
        "Collateral must be fungible"
    );
    assert_eq!(pre[9].account_id, state.pool_id, "Wrong fixed-price pool");
    for (index, owner, token) in [
        (6, &creator, state.token_definition_id),
        (8, &factory, state.token_definition_id),
        (10, &pre[9], state.token_definition_id),
        (11, &pre[9], state.collateral_definition_id),
    ] {
        associated_token_account_core::verify_ata_and_get_seed(
            &pre[index],
            owner,
            token,
            curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        );
    }
    curve_core::validated_config(&pre[13], state.namespace, state.namespace_program_id);
    assert_eq!(state.creation_stage, expected_stage, "Stale creation stage");
    assert!(
        super::trusted_time(&pre[12]) < state.end_timestamp,
        "Sale setup has expired"
    );
    let stage = state.creation_stage;
    if stage == FixedPriceCreationStage::Minted {
        assert_eq!(
            holding_balance(&mint, state.token_definition_id),
            state.total_supply,
            "Mint does not hold the complete launch supply"
        );
        assert_eq!(
            escrow.account,
            Account::default(),
            "Creator escrow already exists"
        );
        if factory_token.account != Account::default() {
            assert_eq!(
                holding_balance(&factory_token, state.token_definition_id),
                0,
                "Factory inventory custody must be empty"
            );
        }
    } else {
        assert_eq!(
            holding_balance(&mint, state.token_definition_id),
            0,
            "Mint must be empty after allocation"
        );
        assert_eq!(
            holding_balance(&factory_token, state.token_definition_id),
            state.sale_inventory,
            "Sale inventory changed"
        );
        if state.creator_allocation != 0 {
            assert_eq!(
                holding_balance(&escrow, state.token_definition_id),
                state.creator_allocation,
                "Creator allocation changed"
            );
        }
    }
    state.creation_stage = match stage {
        FixedPriceCreationStage::Minted => FixedPriceCreationStage::Allocated,
        FixedPriceCreationStage::Allocated => FixedPriceCreationStage::PoolPrepared,
        FixedPriceCreationStage::PoolPrepared => FixedPriceCreationStage::Complete,
        _ => panic!("Unsupported creation stage"),
    };
    if stage == FixedPriceCreationStage::PoolPrepared
        && state.allocation_release == CreatorAllocationRelease::AfterSetup
    {
        state.creator_allocation_released = true;
    }
    let mut post = factory.account.clone();
    post.data = Data::from(&state);
    let snapshot = AccountWithMetadata {
        account: post.clone(),
        ..factory
    };
    let calls = if stage == FixedPriceCreationStage::Minted {
        let mut calls = vec![ChainedCall::new(
            curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            vec![snapshot, definition.clone(), factory_token.clone()],
            &associated_token_account_core::Instruction::Create {
                ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            },
        )];
        let mut sender = AccountWithMetadata {
            is_authorized: true,
            ..mint
        };
        calls.push(
            ChainedCall::new(
                curve_core::TOKEN_PROGRAM_ID,
                vec![
                    sender.clone(),
                    curve_core::after_ata_creation(&factory_token, &definition),
                ],
                &token_core::Instruction::Transfer {
                    amount_to_transfer: state.sale_inventory,
                },
            )
            .with_pda_seeds(vec![super::compute_mint_seed(
                state.namespace,
                state.launch_salt,
            )]),
        );
        if state.creator_allocation != 0 {
            let mut holding = token_core::TokenHolding::try_from(&sender.account.data)
                .expect("Invalid mint holding");
            let token_core::TokenHolding::Fungible { balance, .. } = &mut holding else {
                panic!("Mint must be fungible")
            };
            *balance = balance
                .checked_sub(state.sale_inventory)
                .expect("Missing sale inventory");
            sender.account.data = Data::from(&holding);
            calls.push(
                ChainedCall::new(
                    curve_core::TOKEN_PROGRAM_ID,
                    vec![
                        sender,
                        AccountWithMetadata {
                            is_authorized: true,
                            ..escrow
                        },
                    ],
                    &token_core::Instruction::Transfer {
                        amount_to_transfer: state.creator_allocation,
                    },
                )
                .with_pda_seeds(vec![
                    super::compute_mint_seed(state.namespace, state.launch_salt),
                    super::compute_escrow_seed(state.namespace, state.launch_salt),
                ]),
            );
        }
        calls
    } else {
        let inputs = lbp_core::FixedPricePoolInputs {
            namespace: state.namespace,
            namespace_program_id: state.namespace_program_id,
            factory_program_id: program,
            factory_seed: *super::compute_factory_seed(state.namespace, state.launch_salt)
                .as_bytes(),
            sale_inventory: state.sale_inventory,
            creator_allocation: state.creator_allocation,
            price: state.price,
            start_timestamp: state.start_timestamp,
            end_timestamp: state.end_timestamp,
            creator_allocation_release: state.allocation_release,
        };
        let mut calls = vec![
            ChainedCall::new(
                state.lbp_program_id,
                vec![
                    pre[9].clone(),
                    AccountWithMetadata {
                        is_authorized: true,
                        ..snapshot
                    },
                    definition,
                    pre[7].clone(),
                    factory_token,
                    pre[10].clone(),
                    pre[11].clone(),
                    pre[12].clone(),
                    pre[13].clone(),
                ],
                &if stage == FixedPriceCreationStage::Allocated {
                    lbp_core::Instruction::PrepareFixedPrice { inputs }
                } else {
                    lbp_core::Instruction::ActivateFixedPrice
                },
            )
            .with_pda_seeds(vec![super::compute_factory_seed(
                state.namespace,
                state.launch_salt,
            )]),
        ];
        if stage == FixedPriceCreationStage::Allocated
            && state.allocation_release == CreatorAllocationRelease::AfterSetup
            && state.creator_allocation != 0
        {
            calls.push(ChainedCall::new(
                curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                vec![creator, pre[1].clone(), pre[6].clone()],
                &associated_token_account_core::Instruction::Create {
                    ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
                },
            ));
        }
        if stage == FixedPriceCreationStage::PoolPrepared
            && state.allocation_release == CreatorAllocationRelease::AfterSetup
            && state.creator_allocation != 0
        {
            calls.push(
                ChainedCall::new(
                    curve_core::TOKEN_PROGRAM_ID,
                    vec![
                        AccountWithMetadata {
                            is_authorized: true,
                            ..escrow
                        },
                        pre[6].clone(),
                    ],
                    &token_core::Instruction::Transfer {
                        amount_to_transfer: state.creator_allocation,
                    },
                )
                .with_pda_seeds(vec![super::compute_escrow_seed(
                    state.namespace,
                    state.launch_salt,
                )]),
            );
        }
        calls
    };
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|a| AccountPostState::new(a.account))
        .collect();
    posts[0] = AccountPostState::new(post);
    (posts, calls)
}
