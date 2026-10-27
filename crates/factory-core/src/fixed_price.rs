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
    pub creator_role_id: AccountId,
    pub mode: lbp_core::PoolMode,
    pub creation_stage: FixedPriceCreationStage,
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
pub(super) fn create(input: &ReceiveInput, inputs: FixedPriceInputs) -> Result<Response, Error> {
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
        || inputs.price.collateral_units == 0
        || inputs.price.token_units == 0
        || inputs
            .end_timestamp
            .is_some_and(|end| end <= start_timestamp)
        || inputs.collateral_definition_id == token_definition_id
    {
        return Err(Error::InvalidTerms);
    }
    let state = FixedPriceLaunchState {
        format: *b"LZFXLAUN",
        version: 2,
        creator_role_id: compute_creator_role_pda(inputs.namespace, program, inputs.launch_salt),
        mode: lbp_core::PoolMode::FixedPrice,
        creation_stage: FixedPriceCreationStage::Minted,
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
            command: Message::CreateFixedPriceLbp {
                inputs: inputs.clone(),
            },
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
    expected_stage: FixedPriceCreationStage,
    creator: AccountId,
    now: u64,
) -> Result<Response, Error> {
    let mut state = FixedPriceLaunchState::decode(&input.pre_state)?;
    if input.receiver.account_id
        != compute_factory_pda(
            state.namespace,
            input.receiver.program_account_id,
            state.launch_salt,
        )
    {
        return Err(Error::WrongAccount);
    }
    if state.creation_stage != expected_stage || expected_stage == FixedPriceCreationStage::Complete
    {
        return Err(Error::StaleStage);
    }
    let mint = compute_mint_pda(
        state.namespace,
        input.receiver.program_account_id,
        state.launch_salt,
    );
    state.pending = Some(Pending {
        command: Message::ContinueFixedPriceCreation {
            expected_stage,
            creator,
            now,
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
    if expected_stage == FixedPriceCreationStage::PoolPrepared {
        let pending = state.pending.as_mut().unwrap();
        pending.reads.extend([
            Read::new(
                Actor::new(state.pool_id, state.lbp_program_id),
                &lbp_core::Message::ReadState,
            ),
            holding_read(state.pool_id, state.token_definition_id),
            holding_read(state.pool_id, state.collateral_definition_id),
        ]);
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
    let mut state = FixedPriceLaunchState::decode(&input.pre_state)?;
    if input.receiver.account_id
        != compute_factory_pda(
            state.namespace,
            input.receiver.program_account_id,
            state.launch_salt,
        )
    {
        return Err(Error::WrongAccount);
    }
    if state.creation_stage != FixedPriceCreationStage::Complete {
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
    let mut state = FixedPriceLaunchState::decode(&input.pre_state).ok()?;
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
            if pool.namespace != state.namespace
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
            let clock = if pool.remaining_inventory == 0 {
                None
            } else {
                let end = state.end_timestamp.ok_or(Error::NotClosed)?;
                if now < end {
                    return Err(Error::NotClosed);
                }
                Some(curve_core::clock_assertion(now, None).map_err(|_| Error::InvalidTerms)?)
            };
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
            if expected_stage == FixedPriceCreationStage::PoolPrepared {
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
                balance(
                    &pending.values[6],
                    state.pool_id,
                    state.collateral_definition_id,
                )?;
                state.pending = None;
                state.creation_stage = FixedPriceCreationStage::Complete;
                state.creator_allocation_released =
                    state.allocation_release == CreatorAllocationRelease::AfterSetup;
                let response = transfer(
                    state.response(),
                    input.receiver.account_id,
                    compute_factory_seed(state.namespace, state.launch_salt),
                    state.token_definition_id,
                    state.pool_id,
                    state.sale_inventory,
                )
                .send_sync(
                    Actor::new(state.pool_id, state.lbp_program_id),
                    &lbp_core::Message::ActivateFixedPrice,
                );
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
            if expected_stage == FixedPriceCreationStage::Allocated {
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
                state.creation_stage = FixedPriceCreationStage::PoolPrepared;
                let inputs = lbp_core::FixedPricePoolInputs {
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
                };
                return state.with_setup_deadline(state.response().send_sync(
                    Actor::new(state.pool_id, state.lbp_program_id),
                    &lbp_core::Message::PrepareFixedPrice { inputs },
                ));
            }
            if expected_stage != FixedPriceCreationStage::Minted {
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
            state.creation_stage = FixedPriceCreationStage::Allocated;
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
        let Message::CreateFixedPriceLbp { inputs } = pending.command.clone() else {
            return Err(Error::InvalidMessage);
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
                    &authority::Message::Bootstrap {
                        authority: inputs.creator,
                    },
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
        let (at_least, at_most) = match inputs.start {
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
impl FixedPriceLaunchState {
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
        if state.format != *b"LZFXLAUN" || state.version != 2 {
            return Err(Error::InvalidState);
        }
        Ok(state)
    }
}
