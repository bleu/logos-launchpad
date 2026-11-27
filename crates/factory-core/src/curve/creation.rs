//! Bonding-curve sale mint, allocation, pool preparation and funding.
use super::*;

pub(crate) fn create(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let Message::CreateFactoryPool {
        buy_policy,
        pause_policy,
        ending_policy,
        trade_mode,
        sell_fee_bps,
        per_block_token_limit,
        graduation_hook_program_id,
        graduation_hook_configuration,
        namespace,
        launch_salt,
        name,
        uri,
        sale_reserve,
        graduation_reserve,
        graduation_policy,
        creator_allocation,
        virtual_token_reserve,
        virtual_collateral_reserve,
        end_timestamp,
        curve_program_id,
        collateral_definition_id,
        creator,
        now,
    } = command.clone()
    else {
        return Err(Error::InvalidMessage);
    };
    if input.receiver.account_id
        != compute_factory_pda(namespace, input.receiver.program_account_id, launch_salt)
    {
        return Err(Error::WrongAccount);
    }
    if !input.pre_state.is_empty() {
        return Err(Error::AlreadyInitialized);
    }
    if namespace == AccountId::default()
        || creator == AccountId::default()
        || name.is_empty()
        || uri.is_empty()
        || graduation_hook_program_id.is_none() && !graduation_hook_configuration.is_empty()
    {
        return Err(Error::InvalidTerms);
    }
    SellFeeRate::new(sell_fee_bps).map_err(|_| Error::InvalidTerms)?;
    curve_core::BlockAllocation::new(per_block_token_limit).map_err(|_| Error::InvalidTerms)?;
    validate_curve_policies(ending_policy, end_timestamp)?;
    graduation_policy.validate(graduation_hook_program_id.is_some())?;
    let total_supply = total_supply(sale_reserve, graduation_reserve, creator_allocation)
        .map_err(|_| Error::InvalidTerms)?;
    validate_curve_parameters(
        sale_reserve,
        virtual_token_reserve,
        virtual_collateral_reserve,
    )
    .map_err(|_| Error::InvalidTerms)?;
    let token_definition_id =
        compute_definition_pda(namespace, input.receiver.program_account_id, launch_salt);
    if collateral_definition_id == token_definition_id {
        return Err(Error::InvalidTerms);
    }
    let state = FactoryState {
        buy_policy,
        pause_policy,
        ending_policy,
        trade_mode: TradeMode::try_from(trade_mode).map_err(|_| Error::InvalidTerms)?,
        sell_fee_bps,
        per_block_token_limit,
        graduation_hook: graduation_hook_program_id.map(|program_id| GraduationHook {
            program_id,
            configuration: graduation_hook_configuration,
        }),
        creation_stage: CreationStage::Minted,
        settlement_stage: SettlementStage::Unstarted,
        end_timestamp,
        namespace,
        launch_salt,
        token_definition_id,
        collateral_definition_id,
        sale_reserve,
        graduation_reserve,
        graduation_policy,
        creator_allocation,
        total_supply,
        virtual_token_reserve,
        virtual_collateral_reserve,
        curve_program_id,
        creator_role_id: compute_curve_creator_role_pda(
            namespace,
            input.receiver.program_account_id,
            launch_salt,
        ),
        name,
        uri,
        creator_escrow_id: compute_escrow_pda(
            namespace,
            input.receiver.program_account_id,
            launch_salt,
        ),
        pool_id: curve_core::compute_pool_pda(
            namespace,
            curve_program_id,
            token_definition_id,
            collateral_definition_id,
            input.receiver.account_id,
        ),
        creator_allocation_claimed: false,
        pending: Some(Pending {
            command,
            proof: false,
            reads: vec![
                Read::new(
                    Actor::new(
                        curve_core::compute_config_pda(namespace, curve_program_id),
                        curve_program_id,
                    ),
                    &curve_core::Message::ReadNamespace { namespace },
                ),
                Read::new(
                    Actor::new(collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::ReadState,
                ),
            ],
            values: vec![],
        }),
    };
    state
        .buy_policy
        .validate_binding(state.namespace.value(), state.pool_id.value())
        .map_err(|_| Error::InvalidTerms)?;
    let response = state.next().event(state.event(
        input.receiver.account_id,
        SaleEventKindV1::Created {
            creator: *creator.value(),
            creator_role: *state.creator_role_id.value(),
            token_definition: *state.token_definition_id.value(),
            collateral_definition: *state.collateral_definition_id.value(),
            graduation_reserve: state.graduation_reserve,
            sale_inventory: state.sale_reserve,
            creator_allocation: state.creator_allocation,
            graduation_policy: state.graduation_policy,
            collateral_seed: 0,
            start_timestamp: None,
            end_timestamp: state.end_timestamp,
            policies: EventPoliciesV1 {
                ending: state.ending_policy,
                pause: state.pause_policy,
                sell_fee_bps: state.sell_fee_bps,
            },
        },
    ));
    let advance = Message::ContinueCreation { creator, now };
    Ok(response
        .send_sync(input.receiver, &advance)
        .send_sync(input.receiver, &advance)
        .send_sync(input.receiver, &advance))
}

pub(crate) fn begin(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = FactoryState::decode(&input.pre_state)?;
    state.validate(input.receiver)?;
    let Message::ContinueCreation { creator, .. } = command else {
        return Err(Error::InvalidMessage);
    };
    if state.creation_stage == CreationStage::Active {
        return Err(Error::StaleStage);
    }
    let mint = compute_mint_pda(
        state.namespace,
        input.receiver.program_account_id,
        state.launch_salt,
    );
    state.pending = Some(Pending {
        command,
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
    if state.creation_stage == CreationStage::PoolPrepared {
        state.pending.as_mut().unwrap().reads.extend([
            Read::new(
                Actor::new(state.pool_id, state.curve_program_id),
                &curve_core::Message::ReadState,
            ),
            holding_read(state.pool_id, state.token_definition_id),
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

pub(super) fn finish_mint(
    input: &ReceiveInput,
    state: FactoryState,
    creator: AccountId,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    let config = curve_core::Config::decode(&values[0]).map_err(|_| Error::InvalidState)?;
    if config.namespace != state.namespace {
        return Err(Error::WrongAccount);
    }
    if !matches!(
        TokenState::decode(&values[1])?,
        TokenState::Definition {
            kind: TokenKind::Fungible,
            ..
        }
    ) {
        return Err(Error::InvalidDefinition);
    }
    let mint = token::Message::Create {
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
    };
    let response = curve_core::with_time_bounds(state.response(), None, state.end_timestamp)
        .map_err(|_| Error::InvalidTerms)?;
    Ok(response
        .send(
            SyncMessage::new(
                Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                &authority::Message::BootstrapTransferOnly { authority: creator },
            )
            .with_pda_seeds([compute_curve_creator_role_seed(
                state.namespace,
                state.launch_salt,
            )]),
        )
        .send(
            SyncMessage::new(
                Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &mint,
            )
            .with_pda_seeds([compute_definition_seed(state.namespace, state.launch_salt)]),
        ))
}

pub(super) fn finish_stage(
    input: &ReceiveInput,
    mut state: FactoryState,
    now: u64,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    if TokenState::decode(&values[0])?
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
    if state.creation_stage == CreationStage::PoolPrepared {
        if balance(&values[1], mint, state.token_definition_id)? != state.graduation_reserve
            || balance(
                &values[2],
                input.receiver.account_id,
                state.token_definition_id,
            )? != state.sale_reserve
            || balance(
                &values[3],
                state.creator_escrow_id,
                state.token_definition_id,
            )? != state.creator_allocation
        {
            return Err(Error::InvalidCustody);
        }
        let saved = curve_core::PoolAccount::decode(&values[4]).map_err(|_| Error::InvalidState)?;
        validate_pool(&state, input.receiver, &saved)?;
        if saved.funded
            || saved.pool.k != saved.pool.initial_k
            || saved.pool.virtual_reserve0 != state.virtual_token_reserve
            || saved.pool.virtual_reserve1 != state.virtual_collateral_reserve
            || saved.pool.real_reserve0 != state.sale_reserve
            || saved.pool.real_reserve1 != 0
            || balance(&values[5], state.pool_id, state.token_definition_id)? != 0
        {
            return Err(Error::InvalidCustody);
        }
        state.creation_stage = CreationStage::Active;
        return Ok(transfer(
            state.response(),
            input.receiver.account_id,
            compute_factory_seed(state.namespace, state.launch_salt),
            state.token_definition_id,
            state.pool_id,
            state.sale_reserve,
        )
        .send_sync(
            Actor::new(state.pool_id, state.curve_program_id),
            &curve_core::Message::ActivatePool { now },
        ));
    }
    if state.creation_stage == CreationStage::Allocated {
        if balance(&values[1], mint, state.token_definition_id)? != state.graduation_reserve
            || balance(
                &values[2],
                input.receiver.account_id,
                state.token_definition_id,
            )? != state.sale_reserve
            || balance(
                &values[3],
                state.creator_escrow_id,
                state.token_definition_id,
            )? != state.creator_allocation
        {
            return Err(Error::InvalidCustody);
        }
        state.creation_stage = CreationStage::PoolPrepared;
        return Ok(state.response().send(
            SyncMessage::new(
                Actor::new(input.receiver.account_id, state.curve_program_id),
                &curve_core::Message::CreatePool {
                    buy_policy: state.buy_policy.clone(),
                    pause_policy: state.pause_policy,
                    ending_policy: state.ending_policy,
                    sell_fee_bps: state.sell_fee_bps,
                    per_block_token_limit: state.per_block_token_limit,
                    namespace: state.namespace,
                    token0_definition_id: state.token_definition_id,
                    token1_definition_id: state.collateral_definition_id,
                    allowed_input_token: match state.trade_mode {
                        TradeMode::BuyOnly => Some(1),
                        TradeMode::BuyAndSell => None,
                    },
                    defer_funding: true,
                    token0_amount: state.sale_reserve,
                    token1_amount: 0,
                    virtual_reserve0: state.virtual_token_reserve,
                    virtual_reserve1: state.virtual_collateral_reserve,
                    close_timestamp: state.end_timestamp,
                    close_on_depletion: match state.ending_policy {
                        EndingPolicy::Scheduled => None,
                        _ => Some(0),
                    },
                    owner: input.receiver.account_id,
                    owner_program_id: Some(input.receiver.program_account_id),
                    owner_program_seed: Some(
                        *compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
                    ),
                    curve_program_id: state.curve_program_id,
                    now,
                },
            )
            .with_pda_seeds([compute_factory_seed(state.namespace, state.launch_salt)]),
        ));
    }
    if state.creation_stage != CreationStage::Minted {
        return Err(Error::StaleStage);
    }
    if balance(&values[1], mint, state.token_definition_id)? != state.total_supply
        || balance(
            &values[2],
            input.receiver.account_id,
            state.token_definition_id,
        )? != 0
        || balance(
            &values[3],
            state.creator_escrow_id,
            state.token_definition_id,
        )? != 0
    {
        return Err(Error::InvalidCustody);
    }
    state.creation_stage = CreationStage::Allocated;
    let response = transfer(
        state.response(),
        mint,
        compute_mint_seed(state.namespace, state.launch_salt),
        state.token_definition_id,
        input.receiver.account_id,
        state.sale_reserve,
    );
    Ok(transfer(
        response,
        mint,
        compute_mint_seed(state.namespace, state.launch_salt),
        state.token_definition_id,
        state.creator_escrow_id,
        state.creator_allocation,
    ))
}
