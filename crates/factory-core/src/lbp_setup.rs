//! Shared token, allocation and authority setup for typed LBP sale inputs.
use super::*;
pub use lbp_core::{CreatorAllocationRelease, FixedPrice};
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SaleStart {
    Now { timestamp: u64 },
    At { timestamp: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct FixedPriceInputs {
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub name: String,
    pub uri: String,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub price: FixedPrice,
    pub start: SaleStart,
    pub end_timestamp: Option<u64>,
    pub allocation_release: CreatorAllocationRelease,
    pub namespace_program_id: AccountId,
    pub lbp_program_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub creator: AccountId,
    pub now: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct WeightedInputs {
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub name: String,
    pub uri: String,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub collateral_seed: u128,
    pub token_weight_start: u128,
    pub token_weight_end: u128,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub allocation_release: CreatorAllocationRelease,
    pub namespace_program_id: AccountId,
    pub lbp_program_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub creator: AccountId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum LbpSetupMode {
    FixedPrice,
    Weighted {
        collateral_seed: u128,
        token_weight_start: u128,
        token_weight_end: u128,
    },
}
impl LbpSetupMode {
    fn pool_mode(self) -> lbp_core::PoolMode {
        match self {
            Self::FixedPrice => lbp_core::PoolMode::FixedPrice,
            Self::Weighted {
                collateral_seed,
                token_weight_start,
                token_weight_end,
            } => lbp_core::PoolMode::Weighted {
                collateral_seed,
                token_weight_start,
                token_weight_end,
                pricing_collateral: 0,
            },
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum LbpCreationStage {
    Minted,
    Allocated,
    PoolPrepared,
    Complete,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct LbpLaunchState {
    format: [u8; 8],
    version: u16,
    pub creator_role_id: AccountId,
    pub mode: LbpSetupMode,
    pub creation_stage: LbpCreationStage,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub creator_escrow_id: AccountId,
    pub pool_id: AccountId,
    pub namespace_program_id: AccountId,
    pub lbp_program_id: AccountId,
    pub sale_inventory: u128,
    pub creator_allocation: u128,
    pub total_supply: u128,
    pub price: FixedPrice,
    pub start_timestamp: u64,
    pub end_timestamp: Option<u64>,
    pub allocation_release: CreatorAllocationRelease,
    pub creator_allocation_released: bool,
    pub name: String,
    pub uri: String,
    pub(super) pending: Option<Pending>,
}
struct SetupInputs {
    namespace: AccountId,
    launch_salt: [u8; 32],
    name: String,
    uri: String,
    sale_inventory: u128,
    creator_allocation: u128,
    price: FixedPrice,
    start: SaleStart,
    end_timestamp: Option<u64>,
    allocation_release: CreatorAllocationRelease,
    namespace_program_id: AccountId,
    lbp_program_id: AccountId,
    collateral_definition_id: AccountId,
    creator: AccountId,
    mode: LbpSetupMode,
}
pub(super) fn create_fixed(
    input: &ReceiveInput,
    inputs: FixedPriceInputs,
) -> Result<Response, Error> {
    let command = Message::CreateFixedPriceLbp {
        inputs: inputs.clone(),
    };
    create(
        input,
        SetupInputs {
            namespace: inputs.namespace,
            launch_salt: inputs.launch_salt,
            name: inputs.name,
            uri: inputs.uri,
            sale_inventory: inputs.sale_inventory,
            creator_allocation: inputs.creator_allocation,
            price: inputs.price,
            start: inputs.start,
            end_timestamp: inputs.end_timestamp,
            allocation_release: inputs.allocation_release,
            namespace_program_id: inputs.namespace_program_id,
            lbp_program_id: inputs.lbp_program_id,
            collateral_definition_id: inputs.collateral_definition_id,
            creator: inputs.creator,
            mode: LbpSetupMode::FixedPrice,
        },
        command,
    )
}
pub(super) fn create_weighted(
    input: &ReceiveInput,
    inputs: WeightedInputs,
) -> Result<Response, Error> {
    if inputs.collateral_seed == 0
        || !lbp_core::valid_weight_endpoints(inputs.token_weight_start, inputs.token_weight_end)
    {
        return Err(Error::InvalidTerms);
    }
    let command = Message::CreateWeightedLbp {
        inputs: inputs.clone(),
    };
    create(
        input,
        SetupInputs {
            namespace: inputs.namespace,
            launch_salt: inputs.launch_salt,
            name: inputs.name,
            uri: inputs.uri,
            sale_inventory: inputs.sale_inventory,
            creator_allocation: inputs.creator_allocation,
            price: FixedPrice {
                collateral_units: 0,
                token_units: 0,
            },
            start: SaleStart::At {
                timestamp: inputs.start_timestamp,
            },
            end_timestamp: Some(inputs.end_timestamp),
            allocation_release: inputs.allocation_release,
            namespace_program_id: inputs.namespace_program_id,
            lbp_program_id: inputs.lbp_program_id,
            collateral_definition_id: inputs.collateral_definition_id,
            creator: inputs.creator,
            mode: LbpSetupMode::Weighted {
                collateral_seed: inputs.collateral_seed,
                token_weight_start: inputs.token_weight_start,
                token_weight_end: inputs.token_weight_end,
            },
        },
        command,
    )
}
fn create(input: &ReceiveInput, inputs: SetupInputs, command: Message) -> Result<Response, Error> {
    if input.receiver.account_id
        != compute_factory_pda(
            inputs.namespace,
            input.receiver.program_account_id,
            inputs.launch_salt,
        )
    {
        return Err(Error::WrongAccount);
    }
    if !input.pre_state.is_empty() {
        return Err(Error::AlreadyInitialized);
    }
    let program = input.receiver.program_account_id;
    let token_definition_id = compute_definition_pda(inputs.namespace, program, inputs.launch_salt);
    let start_timestamp = match inputs.start {
        SaleStart::Now { timestamp } | SaleStart::At { timestamp } => timestamp,
    };
    if inputs.namespace == AccountId::default()
        || inputs.creator == AccountId::default()
        || inputs.sale_inventory == 0
        || inputs.name.is_empty()
        || inputs.uri.is_empty()
        || (matches!(inputs.mode, LbpSetupMode::FixedPrice)
            && (inputs.price.collateral_units == 0 || inputs.price.token_units == 0))
        || inputs
            .end_timestamp
            .is_some_and(|end| end <= start_timestamp)
        || inputs.collateral_definition_id == token_definition_id
    {
        return Err(Error::InvalidTerms);
    }
    let state = LbpLaunchState {
        format: match inputs.mode {
            LbpSetupMode::FixedPrice => *b"LZFXLAUN",
            LbpSetupMode::Weighted { .. } => *b"LZWGLAUN",
        },
        version: 2,
        creator_role_id: compute_creator_role_pda(inputs.namespace, program, inputs.launch_salt),
        mode: inputs.mode,
        creation_stage: LbpCreationStage::Minted,
        namespace: inputs.namespace,
        launch_salt: inputs.launch_salt,
        token_definition_id,
        collateral_definition_id: inputs.collateral_definition_id,
        creator_escrow_id: compute_escrow_pda(inputs.namespace, program, inputs.launch_salt),
        pool_id: lbp_core::compute_pool_pda(
            inputs.namespace,
            inputs.lbp_program_id,
            token_definition_id,
            inputs.collateral_definition_id,
            input.receiver.account_id,
        ),
        namespace_program_id: inputs.namespace_program_id,
        lbp_program_id: inputs.lbp_program_id,
        sale_inventory: inputs.sale_inventory,
        creator_allocation: inputs.creator_allocation,
        total_supply: inputs
            .sale_inventory
            .checked_add(inputs.creator_allocation)
            .ok_or(Error::Overflow)?,
        price: inputs.price,
        start_timestamp,
        end_timestamp: inputs.end_timestamp,
        allocation_release: inputs.allocation_release,
        creator_allocation_released: false,
        name: inputs.name.clone(),
        uri: inputs.uri.clone(),
        pending: Some(Pending {
            command,
            proof: false,
            reads: vec![
                Read::new(
                    Actor::new(
                        curve_core::compute_config_pda(
                            inputs.namespace,
                            inputs.namespace_program_id,
                        ),
                        inputs.namespace_program_id,
                    ),
                    &curve_core::Message::ReadNamespace {
                        namespace: inputs.namespace,
                    },
                ),
                Read::new(
                    Actor::new(inputs.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::ReadState,
                ),
            ],
            values: vec![],
        }),
    };
    Ok(state.next())
}
pub(super) fn begin(
    input: &ReceiveInput,
    expected_stage: LbpCreationStage,
    creator: AccountId,
    now: u64,
    weighted: bool,
) -> Result<Response, Error> {
    let mut state = LbpLaunchState::decode(&input.pre_state)?;
    if matches!(state.mode, LbpSetupMode::Weighted { .. }) != weighted {
        return Err(Error::InvalidState);
    }
    if input.receiver.account_id
        != compute_factory_pda(
            state.namespace,
            input.receiver.program_account_id,
            state.launch_salt,
        )
    {
        return Err(Error::WrongAccount);
    }
    if state.creation_stage != expected_stage || expected_stage == LbpCreationStage::Complete {
        return Err(Error::StaleStage);
    }
    let mint = compute_mint_pda(
        state.namespace,
        input.receiver.program_account_id,
        state.launch_salt,
    );
    state.pending = Some(Pending {
        command: if weighted {
            Message::ContinueWeightedCreation {
                expected_stage,
                creator,
            }
        } else {
            Message::ContinueFixedPriceCreation {
                expected_stage,
                creator,
                now,
            }
        },
        proof: true,
        reads: vec![
            Read::new(
                Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::ReadState,
            ),
            holding_read(mint, state.token_definition_id),
            holding_read(input.receiver.account_id, state.token_definition_id),
            holding_read(state.creator_escrow_id, state.token_definition_id),
        ],
        values: vec![],
    });
    if expected_stage == LbpCreationStage::PoolPrepared {
        let pending = state.pending.as_mut().unwrap();
        pending.reads.extend([
            Read::new(
                Actor::new(state.pool_id, state.lbp_program_id),
                &lbp_core::Message::ReadState,
            ),
            holding_read(state.pool_id, state.token_definition_id),
            holding_read(state.pool_id, state.collateral_definition_id),
        ]);
        if weighted {
            pending
                .reads
                .push(holding_read(creator, state.collateral_definition_id));
        }
    }
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}
pub(super) fn begin_payout(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = LbpLaunchState::decode(&input.pre_state)?;
    if input.receiver.account_id
        != compute_factory_pda(
            state.namespace,
            input.receiver.program_account_id,
            state.launch_salt,
        )
    {
        return Err(Error::WrongAccount);
    }
    if state.mode != LbpSetupMode::FixedPrice {
        return Err(Error::InvalidState);
    }
    if state.creation_stage != LbpCreationStage::Complete {
        return Err(Error::CreationPending);
    }
    let creator = match command {
        Message::WithdrawFactoryProceeds { creator, .. }
        | Message::ClaimCreatorAllocation { creator, .. } => creator,
        _ => return Err(Error::InvalidMessage),
    };
    let mut reads = vec![];
    if matches!(command, Message::ClaimCreatorAllocation { .. }) {
        reads.push(Read::new(
            Actor::new(state.pool_id, state.lbp_program_id),
            &lbp_core::Message::ReadState,
        ));
        if state.creator_allocation != 0 {
            reads.push(holding_read(
                state.creator_escrow_id,
                state.token_definition_id,
            ));
        }
    }
    state.pending = Some(Pending {
        command,
        proof: true,
        reads,
        values: vec![],
    });
    Ok(state.response().send_sync(
        Actor::new(creator, AUTHORITY_PROGRAM_ACCOUNT_ID),
        &authority::Message::Check {
            role: state.creator_role_id,
            callback: input.receiver,
        },
    ))
}

pub(super) fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    let mut state = LbpLaunchState::decode(&input.pre_state).ok()?;
    let mut pending = state.pending.take()?;
    Some((|| {
        if input.receiver.account_id
            != compute_factory_pda(
                state.namespace,
                input.receiver.program_account_id,
                state.launch_salt,
            )
        {
            return Err(Error::WrongAccount);
        }
        if pending.proof {
            let creator = match pending.command {
                Message::ContinueFixedPriceCreation { creator, .. }
                | Message::ContinueWeightedCreation { creator, .. }
                | Message::ClaimCreatorAllocation { creator, .. }
                | Message::WithdrawFactoryProceeds { creator, .. } => creator,
                _ => return Err(Error::InvalidState),
            };
            authority::verify(
                input,
                Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                creator,
            )?;
            if let Message::WithdrawFactoryProceeds { creator, now } = pending.command {
                return Ok(state.response().send_sync(
                    Actor::new(state.pool_id, state.lbp_program_id),
                    &lbp_core::Message::SettleFixedPrice { creator, now },
                ));
            }
            if matches!(pending.command, Message::ClaimCreatorAllocation { .. }) {
                if state.creator_allocation_released {
                    return Ok(state.response());
                }
                pending.proof = false;
                state.pending = Some(pending);
                return Ok(state.next());
            }
            pending.proof = false;
            state.pending = Some(pending);
            return Ok(state
                .response()
                .send_sync(
                    Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::Initialize {
                        owner: input.receiver.account_id,
                    },
                )
                .send_sync(
                    Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::Initialize {
                        owner: state.creator_escrow_id,
                    },
                )
                .send(state.next().sync_messages.remove(0)));
        }
        let expected = pending
            .reads
            .get(pending.values.len())
            .ok_or(Error::InvalidState)?;
        if input.from != Some(expected.actor) {
            return Err(Error::Unauthorized);
        }
        let reply: StateReply =
            borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
        pending.values.push(reply.state.to_vec());
        if pending.values.len() < pending.reads.len() {
            state.pending = Some(pending);
            return Ok(state.next());
        }
        if let Message::ClaimCreatorAllocation { creator, now } = pending.command {
            let pool = lbp_core::PoolAccount::decode(&pending.values[0])
                .map_err(|_| Error::InvalidState)?;
            if pool.mode != lbp_core::PoolMode::FixedPrice
                || pool.namespace != state.namespace
                || pool.namespace_program_id != state.namespace_program_id
                || pool.factory_owner != input.receiver.account_id
                || pool.factory_program_id != input.receiver.program_account_id
                || pool.factory_seed
                    != *compute_factory_seed(state.namespace, state.launch_salt).as_bytes()
                || pool.token_definition_id != state.token_definition_id
                || pool.collateral_definition_id != state.collateral_definition_id
                || pool.sale_inventory != state.sale_inventory
                || pool.creator_allocation != state.creator_allocation
                || pool.price != state.price
                || pool.start_timestamp != state.start_timestamp
                || pool.end_timestamp != state.end_timestamp
                || pool.creator_allocation_release != state.allocation_release
            {
                return Err(Error::InvalidState);
            }
            if !pool.funded {
                return Err(Error::CreationPending);
            }
            let clock = pool
                .close_clock_assertion(now)
                .map_err(|error| match error {
                    lbp_core::Error::NotClosed => Error::NotClosed,
                    _ => Error::InvalidTerms,
                })?;
            if state.creator_allocation != 0
                && balance(
                    &pending.values[1],
                    state.creator_escrow_id,
                    state.token_definition_id,
                )? < state.creator_allocation
            {
                return Err(Error::InvalidCustody);
            }
            state.creator_allocation_released = true;
            let mut response = state.response();
            if let Some(clock) = clock {
                response = response.send_sync(
                    Actor::new(
                        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                        clock_core::clock_account_id(),
                    ),
                    &clock,
                );
            }
            return Ok(transfer(
                response,
                state.creator_escrow_id,
                compute_escrow_seed(state.namespace, state.launch_salt),
                state.token_definition_id,
                creator,
                state.creator_allocation,
            ));
        }
        if let Message::ContinueFixedPriceCreation {
            expected_stage,
            creator,
            ..
        }
        | Message::ContinueWeightedCreation {
            expected_stage,
            creator,
        } = pending.command
        {
            if TokenState::decode(&pending.values[0])?
                != (TokenState::Definition {
                    name: state.name.clone(),
                    kind: TokenKind::Fungible,
                    supply: state.total_supply,
                    metadata: Some(token::Metadata {
                        standard: token::MetadataStandard::Simple,
                        uri: state.uri.clone(),
                        creators: String::new(),
                    }),
                })
            {
                return Err(Error::InvalidDefinition);
            }
            let mint = compute_mint_pda(
                state.namespace,
                input.receiver.program_account_id,
                state.launch_salt,
            );
            if expected_stage == LbpCreationStage::PoolPrepared {
                if balance(&pending.values[1], mint, state.token_definition_id)? != 0
                    || balance(
                        &pending.values[2],
                        input.receiver.account_id,
                        state.token_definition_id,
                    )? != state.sale_inventory
                    || balance(
                        &pending.values[3],
                        state.creator_escrow_id,
                        state.token_definition_id,
                    )? != state.creator_allocation
                {
                    return Err(Error::InvalidCustody);
                }
                let pool = lbp_core::PoolAccount::decode(&pending.values[4])
                    .map_err(|_| Error::InvalidState)?;
                if pool.funded
                    || pool.mode != state.mode.pool_mode()
                    || pool.remaining_inventory != state.sale_inventory
                    || pool.accrued_fees != 0
                    || pool.recorded_collateral != 0
                    || pool.namespace != state.namespace
                    || pool.namespace_program_id != state.namespace_program_id
                    || pool.factory_owner != input.receiver.account_id
                    || pool.factory_program_id != input.receiver.program_account_id
                    || pool.factory_seed
                        != *compute_factory_seed(state.namespace, state.launch_salt).as_bytes()
                    || pool.token_definition_id != state.token_definition_id
                    || pool.collateral_definition_id != state.collateral_definition_id
                    || pool.sale_inventory != state.sale_inventory
                    || pool.creator_allocation != state.creator_allocation
                    || pool.price != state.price
                    || pool.start_timestamp != state.start_timestamp
                    || pool.end_timestamp != state.end_timestamp
                    || pool.creator_allocation_release != state.allocation_release
                {
                    return Err(Error::InvalidState);
                }
                if balance(&pending.values[5], state.pool_id, state.token_definition_id)? != 0 {
                    return Err(Error::InvalidCustody);
                }
                let pool_collateral = balance(
                    &pending.values[6],
                    state.pool_id,
                    state.collateral_definition_id,
                )?;
                if let LbpSetupMode::Weighted {
                    collateral_seed, ..
                } = state.mode
                {
                    if balance(&pending.values[7], creator, state.collateral_definition_id)?
                        < collateral_seed
                    {
                        return Err(Error::InvalidCustody);
                    }
                    pool_collateral
                        .checked_add(collateral_seed)
                        .ok_or(Error::Overflow)?;
                }
                state.pending = None;
                state.creation_stage = LbpCreationStage::Complete;
                state.creator_allocation_released =
                    state.allocation_release == CreatorAllocationRelease::AfterSetup;
                let response = transfer(
                    state.response(),
                    input.receiver.account_id,
                    compute_factory_seed(state.namespace, state.launch_salt),
                    state.token_definition_id,
                    state.pool_id,
                    state.sale_inventory,
                );
                let response = if let LbpSetupMode::Weighted {
                    collateral_seed, ..
                } = state.mode
                {
                    response.send_sync(
                        Actor::new(creator, TOKEN_PROGRAM_ACCOUNT_ID),
                        &token::Message::Transfer {
                            definition: state.collateral_definition_id,
                            recipient: state.pool_id,
                            amount: collateral_seed,
                        },
                    )
                } else {
                    response
                };
                let activation = match state.mode {
                    LbpSetupMode::FixedPrice => lbp_core::Message::ActivateFixedPrice,
                    LbpSetupMode::Weighted { .. } => lbp_core::Message::ActivateWeighted,
                };
                let response = response
                    .send_sync(Actor::new(state.pool_id, state.lbp_program_id), &activation);
                let response = state.with_setup_deadline(response)?;
                return Ok(if state.creator_allocation_released {
                    transfer(
                        response,
                        state.creator_escrow_id,
                        compute_escrow_seed(state.namespace, state.launch_salt),
                        state.token_definition_id,
                        creator,
                        state.creator_allocation,
                    )
                } else {
                    response
                });
            }
            if expected_stage == LbpCreationStage::Allocated {
                if balance(&pending.values[1], mint, state.token_definition_id)? != 0
                    || balance(
                        &pending.values[2],
                        input.receiver.account_id,
                        state.token_definition_id,
                    )? != state.sale_inventory
                    || balance(
                        &pending.values[3],
                        state.creator_escrow_id,
                        state.token_definition_id,
                    )? != state.creator_allocation
                {
                    return Err(Error::InvalidCustody);
                }
                state.pending = None;
                state.creation_stage = LbpCreationStage::PoolPrepared;
                let prepare = match state.mode {
                    LbpSetupMode::FixedPrice => lbp_core::Message::PrepareFixedPrice {
                        inputs: lbp_core::FixedPricePoolInputs {
                            namespace: state.namespace,
                            namespace_program_id: state.namespace_program_id,
                            factory_program_id: input.receiver.program_account_id,
                            factory_seed: *compute_factory_seed(state.namespace, state.launch_salt)
                                .as_bytes(),
                            token_definition_id: state.token_definition_id,
                            collateral_definition_id: state.collateral_definition_id,
                            sale_inventory: state.sale_inventory,
                            creator_allocation: state.creator_allocation,
                            price: state.price,
                            start_timestamp: state.start_timestamp,
                            end_timestamp: state.end_timestamp,
                            creator_allocation_release: state.allocation_release,
                        },
                    },
                    LbpSetupMode::Weighted {
                        collateral_seed,
                        token_weight_start,
                        token_weight_end,
                    } => lbp_core::Message::PrepareWeighted {
                        inputs: lbp_core::WeightedPoolInputs {
                            namespace: state.namespace,
                            namespace_program_id: state.namespace_program_id,
                            factory_program_id: input.receiver.program_account_id,
                            factory_seed: *compute_factory_seed(state.namespace, state.launch_salt)
                                .as_bytes(),
                            token_definition_id: state.token_definition_id,
                            collateral_definition_id: state.collateral_definition_id,
                            sale_inventory: state.sale_inventory,
                            creator_allocation: state.creator_allocation,
                            collateral_seed,
                            token_weight_start,
                            token_weight_end,
                            start_timestamp: state.start_timestamp,
                            end_timestamp: state.end_timestamp.ok_or(Error::InvalidState)?,
                            creator_allocation_release: state.allocation_release,
                        },
                    },
                };
                return state.with_setup_deadline(
                    state
                        .response()
                        .send_sync(Actor::new(state.pool_id, state.lbp_program_id), &prepare),
                );
            }
            if expected_stage != LbpCreationStage::Minted {
                return Err(Error::StaleStage);
            }
            if balance(&pending.values[1], mint, state.token_definition_id)? != state.total_supply
                || balance(
                    &pending.values[2],
                    input.receiver.account_id,
                    state.token_definition_id,
                )? != 0
                || balance(
                    &pending.values[3],
                    state.creator_escrow_id,
                    state.token_definition_id,
                )? != 0
            {
                return Err(Error::InvalidCustody);
            }
            state.pending = None;
            state.creation_stage = LbpCreationStage::Allocated;
            let response = transfer(
                state.response(),
                mint,
                compute_mint_seed(state.namespace, state.launch_salt),
                state.token_definition_id,
                input.receiver.account_id,
                state.sale_inventory,
            );
            return state.with_setup_deadline(transfer(
                response,
                mint,
                compute_mint_seed(state.namespace, state.launch_salt),
                state.token_definition_id,
                state.creator_escrow_id,
                state.creator_allocation,
            ));
        }
        let (creator, start) = match pending.command {
            Message::CreateFixedPriceLbp { inputs } => (inputs.creator, inputs.start),
            Message::CreateWeightedLbp { inputs } => (
                inputs.creator,
                SaleStart::At {
                    timestamp: inputs.start_timestamp,
                },
            ),
            _ => return Err(Error::InvalidMessage),
        };
        let config =
            curve_core::Config::decode(&pending.values[0]).map_err(|_| Error::InvalidState)?;
        if config.namespace != state.namespace {
            return Err(Error::WrongAccount);
        }
        if !matches!(
            TokenState::decode(&pending.values[1])?,
            TokenState::Definition {
                kind: TokenKind::Fungible,
                ..
            }
        ) {
            return Err(Error::InvalidDefinition);
        }
        state.pending = None;
        let response = state
            .response()
            .send(
                SyncMessage::new(
                    Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                    &authority::Message::Bootstrap { authority: creator },
                )
                .with_pda_seeds([compute_creator_role_seed(
                    state.namespace,
                    state.launch_salt,
                )]),
            )
            .send(
                SyncMessage::new(
                    Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::Create {
                        name: state.name.clone(),
                        kind: TokenKind::Fungible,
                        supply: state.total_supply,
                        recipient: compute_mint_pda(
                            state.namespace,
                            input.receiver.program_account_id,
                            state.launch_salt,
                        ),
                        metadata: Some(token::Metadata {
                            standard: token::MetadataStandard::Simple,
                            uri: state.uri.clone(),
                            creators: String::new(),
                        }),
                    },
                )
                .with_pda_seeds([compute_definition_seed(state.namespace, state.launch_salt)]),
            );
        let (at_least, at_most) = match start {
            SaleStart::Now { timestamp } => (
                curve_core::timestamp_millis(timestamp).map_err(|_| Error::InvalidTerms)?,
                u64::MAX,
            ),
            SaleStart::At { timestamp } => (
                0,
                curve_core::timestamp_millis(timestamp).map_err(|_| Error::InvalidTerms)?,
            ),
        };
        state.with_setup_deadline(response.send_sync(
            Actor::new(
                clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                clock_core::clock_account_id(),
            ),
            &clock_core::Message::AssertTimestamp { at_least, at_most },
        ))
    })())
}
impl LbpLaunchState {
    fn with_setup_deadline(&self, response: Response) -> Result<Response, Error> {
        let Some(end_timestamp) = self.end_timestamp else {
            return Ok(response);
        };
        Ok(response.send_sync(
            Actor::new(
                clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
                clock_core::clock_account_id(),
            ),
            &curve_core::clock_assertion(0, Some(end_timestamp))
                .map_err(|_| Error::InvalidTerms)?,
        ))
    }
    fn response(&self) -> Response {
        Response::set_state(borsh::to_vec(self).unwrap())
    }
    fn next(&self) -> Response {
        let pending = self.pending.as_ref().unwrap();
        let read = &pending.reads[pending.values.len()];
        let mut message = SyncMessage::new(read.actor, &());
        message.message = read.message.clone();
        self.response().send(message)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let state: Self = borsh::from_slice(bytes).map_err(|_| Error::InvalidState)?;
        let expected = match state.mode {
            LbpSetupMode::FixedPrice => *b"LZFXLAUN",
            LbpSetupMode::Weighted { .. } => *b"LZWGLAUN",
        };
        if state.format != expected || state.version != 2 {
            return Err(Error::InvalidState);
        }
        Ok(state)
    }
}
