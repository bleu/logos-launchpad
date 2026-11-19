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
    pub buy_policy: curve_core::BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub graduation_hook: Option<GraduationHook>,
    pub graduation_policy: GraduationPolicy,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub name: String,
    pub uri: String,
    pub sale_inventory: u128,
    pub graduation_reserve: u128,
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
    pub buy_policy: curve_core::BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub graduation_hook: Option<GraduationHook>,
    pub graduation_policy: GraduationPolicy,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub name: String,
    pub uri: String,
    pub sale_inventory: u128,
    pub graduation_reserve: u128,
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
                checkpoint_time: 0,
                checkpoint_token_weight: 0,
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
    pub protocol_fee_bps: u16,
    pub buy_policy: curve_core::BuyPolicy,
    pub pause_policy: PausePolicy,
    pub ending_policy: EndingPolicy,
    pub trade_mode: TradeMode,
    pub sell_fee_bps: u16,
    pub graduation_hook: Option<GraduationHook>,
    pub graduation_policy: GraduationPolicy,
    format: [u8; 8],
    version: u16,
    pub creator_role_id: AccountId,
    pub mode: LbpSetupMode,
    pub creation_stage: LbpCreationStage,
    pub settlement_progress: SettlementProgress,
    pub namespace: AccountId,
    pub launch_salt: [u8; 32],
    pub token_definition_id: AccountId,
    pub collateral_definition_id: AccountId,
    pub creator_escrow_id: AccountId,
    pub pool_id: AccountId,
    pub namespace_program_id: AccountId,
    pub lbp_program_id: AccountId,
    pub sale_inventory: u128,
    pub graduation_reserve: u128,
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
    settlement_pending: Option<LbpSettlementPhase>,
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
enum LbpSettlementPhase {
    AdapterHoldings {
        pool: lbp_core::PoolAccount,
        creator_hint: AccountId,
        step: u8,
    },
    Creator {
        pool: lbp_core::PoolAccount,
        creator_hint: AccountId,
    },
    PoolApplied {
        receipt: SettlementReceipt,
    },
    AdapterResult {
        receipt: SettlementReceipt,
        request_hash: [u8; 32],
    },
    PoolCompleted {
        receipt_hash: [u8; 32],
        progress: SettlementProgress,
        graduation_amounts: Option<SettlementAmounts>,
    },
}

struct SetupInputs {
    buy_policy: curve_core::BuyPolicy,
    pause_policy: PausePolicy,
    ending_policy: EndingPolicy,
    trade_mode: TradeMode,
    sell_fee_bps: u16,
    graduation_hook: Option<GraduationHook>,
    graduation_policy: GraduationPolicy,
    namespace: AccountId,
    launch_salt: [u8; 32],
    name: String,
    uri: String,
    sale_inventory: u128,
    graduation_reserve: u128,
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
            buy_policy: inputs.buy_policy,
            pause_policy: inputs.pause_policy,
            ending_policy: inputs.ending_policy,
            trade_mode: inputs.trade_mode,
            sell_fee_bps: inputs.sell_fee_bps,
            graduation_hook: inputs.graduation_hook,
            graduation_policy: inputs.graduation_policy,
            namespace: inputs.namespace,
            launch_salt: inputs.launch_salt,
            name: inputs.name,
            uri: inputs.uri,
            sale_inventory: inputs.sale_inventory,
            graduation_reserve: inputs.graduation_reserve,
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
            buy_policy: inputs.buy_policy,
            pause_policy: inputs.pause_policy,
            ending_policy: inputs.ending_policy,
            trade_mode: inputs.trade_mode,
            sell_fee_bps: inputs.sell_fee_bps,
            graduation_hook: inputs.graduation_hook,
            graduation_policy: inputs.graduation_policy,
            namespace: inputs.namespace,
            launch_salt: inputs.launch_salt,
            name: inputs.name,
            uri: inputs.uri,
            sale_inventory: inputs.sale_inventory,
            graduation_reserve: inputs.graduation_reserve,
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
fn validate_lbp_policies(
    mode: LbpSetupMode,
    ending: EndingPolicy,
    start: u64,
    end: Option<u64>,
) -> Result<(), Error> {
    if !matches!(ending, EndingPolicy::Scheduled | EndingPolicy::Either)
        && !(mode == LbpSetupMode::FixedPrice && ending == EndingPolicy::Depletion)
    {
        return Err(Error::InvalidTerms);
    }
    validate_window(ending, Some(start), end)
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
    if inputs
        .graduation_hook
        .as_ref()
        .is_some_and(|hook| hook.program_id == AccountId::default())
    {
        return Err(Error::InvalidTerms);
    }
    inputs
        .graduation_policy
        .validate(inputs.graduation_hook.is_some())
        .map_err(|_| Error::InvalidTerms)?;
    SellFeeRate::new(inputs.sell_fee_bps).map_err(|_| Error::InvalidTerms)?;
    validate_lbp_policies(
        inputs.mode,
        inputs.ending_policy,
        start_timestamp,
        inputs.end_timestamp,
    )?;
    let pool_id = lbp_core::compute_pool_pda(
        inputs.namespace,
        inputs.lbp_program_id,
        token_definition_id,
        inputs.collateral_definition_id,
        input.receiver.account_id,
    );
    inputs
        .buy_policy
        .validate_binding(inputs.namespace.value(), pool_id.value())
        .map_err(|_| Error::InvalidTerms)?;
    let state = LbpLaunchState {
        protocol_fee_bps: 0,
        buy_policy: inputs.buy_policy,
        pause_policy: inputs.pause_policy,
        ending_policy: inputs.ending_policy,
        trade_mode: inputs.trade_mode,
        sell_fee_bps: inputs.sell_fee_bps,
        graduation_hook: inputs.graduation_hook,
        graduation_policy: inputs.graduation_policy,
        format: match inputs.mode {
            LbpSetupMode::FixedPrice => *b"LZFXLAUN",
            LbpSetupMode::Weighted { .. } => *b"LZWGLAUN",
        },
        version: 6,
        creator_role_id: compute_creator_role_pda(inputs.namespace, program, inputs.launch_salt),
        mode: inputs.mode,
        creation_stage: LbpCreationStage::Minted,
        settlement_progress: SettlementProgress::Unselected,
        settlement_pending: None,
        namespace: inputs.namespace,
        launch_salt: inputs.launch_salt,
        token_definition_id,
        collateral_definition_id: inputs.collateral_definition_id,
        creator_escrow_id: compute_escrow_pda(inputs.namespace, program, inputs.launch_salt),
        pool_id,
        namespace_program_id: inputs.namespace_program_id,
        lbp_program_id: inputs.lbp_program_id,
        sale_inventory: inputs.sale_inventory,
        graduation_reserve: inputs.graduation_reserve,
        creator_allocation: inputs.creator_allocation,
        total_supply: inputs
            .sale_inventory
            .checked_add(inputs.graduation_reserve)
            .and_then(|amount| amount.checked_add(inputs.creator_allocation))
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
    Ok(state.next().event(state.event(
        input.receiver.account_id,
        SaleEventKindV1::Created {
            creator: *inputs.creator.value(),
            creator_role: *state.creator_role_id.value(),
            token_definition: *state.token_definition_id.value(),
            collateral_definition: *state.collateral_definition_id.value(),
            graduation_reserve: state.graduation_reserve,
            sale_inventory: state.sale_inventory,
            creator_allocation: state.creator_allocation,
            graduation_policy: state.graduation_policy,
            collateral_seed: match state.mode {
                LbpSetupMode::FixedPrice => 0,
                LbpSetupMode::Weighted {
                    collateral_seed, ..
                } => collateral_seed,
            },
            start_timestamp: Some(state.start_timestamp),
            end_timestamp: state.end_timestamp,
            policies: EventPoliciesV1 {
                ending: state.ending_policy,
                pause: state.pause_policy,
                sell_fee_bps: state.sell_fee_bps,
            },
        },
    )))
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
    let weighted = matches!(
        command,
        Message::SetWeightedPaused { .. }
            | Message::WithdrawWeightedProceeds { .. }
            | Message::ClaimWeightedAllocation { .. }
    );
    if !matches!(command, Message::SetPaused { .. })
        && matches!(state.mode, LbpSetupMode::Weighted { .. }) != weighted
    {
        return Err(Error::InvalidState);
    }
    if state.creation_stage != LbpCreationStage::Complete {
        return Err(Error::CreationPending);
    }
    let creator = match command {
        Message::WithdrawFactoryProceeds { creator, .. }
        | Message::ClaimCreatorAllocation { creator, .. }
        | Message::SetPaused { creator, .. }
        | Message::SetWeightedPaused { creator, .. }
        | Message::WithdrawWeightedProceeds { creator }
        | Message::ClaimWeightedAllocation { creator } => creator,
        _ => return Err(Error::InvalidMessage),
    };
    if matches!(
        command,
        Message::WithdrawFactoryProceeds { .. } | Message::WithdrawWeightedProceeds { .. }
    ) {
        state
            .graduation_policy
            .validate_route(SettlementRoute::Manual, state.graduation_hook.is_some())
            .map_err(|_| Error::InvalidTerms)?;
    }
    let mut reads = vec![];
    if matches!(
        command,
        Message::ClaimCreatorAllocation { .. }
            | Message::ClaimWeightedAllocation { .. }
            | Message::WithdrawFactoryProceeds { .. }
            | Message::WithdrawWeightedProceeds { .. }
    ) {
        reads.push(Read::new(
            Actor::new(state.pool_id, state.lbp_program_id),
            &lbp_core::Message::ReadState,
        ));
        if matches!(
            command,
            Message::ClaimCreatorAllocation { .. } | Message::ClaimWeightedAllocation { .. }
        ) && state.creator_allocation != 0
        {
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
    let mut state: LbpLaunchState = borsh::from_slice(&input.pre_state).ok()?;
    if state.settlement_pending.is_some() {
        return Some(resume_settlement(input, state));
    }
    let mut pending = state.pending.take()?;
    Some((|| {
        state.validate_terms()?;
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
                | Message::WithdrawFactoryProceeds { creator, .. }
                | Message::SetPaused { creator, .. }
                | Message::SetWeightedPaused { creator, .. }
                | Message::WithdrawWeightedProceeds { creator }
                | Message::ClaimWeightedAllocation { creator } => creator,
                _ => return Err(Error::InvalidState),
            };
            authority::verify(
                input,
                Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                creator,
            )?;
            if let Message::SetPaused { paused, .. } = pending.command {
                return Ok(state.response().send(
                    SyncMessage::new(
                        Actor::new(state.pool_id, state.lbp_program_id),
                        &lbp_core::Message::SetPaused { paused },
                    )
                    .with_pda_seeds([compute_factory_seed(state.namespace, state.launch_salt)]),
                ));
            }
            if let Message::SetWeightedPaused { paused, .. } = pending.command {
                return Ok(state.response().send_sync(
                    Actor::new(state.pool_id, state.lbp_program_id),
                    &lbp_core::Message::SetWeightedPaused { paused },
                ));
            }
            if matches!(
                pending.command,
                Message::WithdrawWeightedProceeds { .. } | Message::WithdrawFactoryProceeds { .. }
            ) {
                pending.proof = false;
                state.pending = Some(pending);
                return Ok(state.next());
            }
            if matches!(
                pending.command,
                Message::ClaimCreatorAllocation { .. } | Message::ClaimWeightedAllocation { .. }
            ) {
                if state.creator_allocation_released
                    && state.allocation_release == CreatorAllocationRelease::AfterSetup
                {
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
        if matches!(pending.command, Message::ExecuteGraduationHook { .. })
            && pending.values.len() == 1
        {
            let pool = lbp_core::PoolAccount::decode(&pending.values[0])
                .map_err(|_| Error::InvalidState)?;
            state.validate_settlement_pool(input.receiver, &pool)?;
            state
                .settlement_progress
                .ensure_available(SettlementRoute::Graduation)?;
            let expiry = pool.close_time_bound().map_err(|error| match error {
                lbp_core::Error::NotClosed => Error::NotClosed,
                _ => Error::InvalidTerms,
            })?;
            state.pending = Some(pending);
            return curve_core::with_time_bounds(state.next(), expiry, None)
                .map_err(|_| Error::InvalidTerms);
        }
        if pending.values.len() < pending.reads.len() {
            state.pending = Some(pending);
            return Ok(state.next());
        }
        if matches!(
            pending.command,
            Message::ExecuteGraduationHook { .. }
                | Message::WithdrawFactoryProceeds { .. }
                | Message::WithdrawWeightedProceeds { .. }
        ) {
            return settlement_ready(input, state, pending);
        }
        if let Message::ClaimCreatorAllocation { creator, .. }
        | Message::ClaimWeightedAllocation { creator } = pending.command
        {
            let pool = lbp_core::PoolAccount::decode(&pending.values[0])
                .map_err(|_| Error::InvalidState)?;
            if pool.graduation_policy != state.graduation_policy
                || pool.graduation_adapter != state.adapter(input.receiver)
                || pool.ending_policy != state.ending_policy
                || pool.trade_mode != state.trade_mode
                || pool.protocol_fee_bps != state.protocol_fee_bps
                || pool.sell_fee_bps != state.sell_fee_bps
            {
                return Err(Error::InvalidCustody);
            }
            if pool.buy_policy != state.buy_policy
                || pool.trade_control.policy() != state.pause_policy
                || !payout_mode_matches(state.mode, pool.mode)
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
            let expiry = pool.close_time_bound().map_err(|error| match error {
                lbp_core::Error::NotClosed => Error::NotClosed,
                _ => Error::InvalidTerms,
            })?;
            if state.creator_allocation_released {
                return curve_core::with_time_bounds(state.response(), expiry, None)
                    .map_err(|_| Error::InvalidTerms);
            }
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
            let response = curve_core::with_time_bounds(state.response(), expiry, None)
                .map_err(|_| Error::InvalidTerms)?;
            return Ok(state.allocation_event(
                transfer(
                    response,
                    state.creator_escrow_id,
                    compute_escrow_seed(state.namespace, state.launch_salt),
                    state.token_definition_id,
                    creator,
                    state.creator_allocation,
                ),
                input.receiver.account_id,
                creator,
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
                if balance(&pending.values[1], mint, state.token_definition_id)?
                    != state.graduation_reserve
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
                if pool.graduation_policy != state.graduation_policy
                    || pool.graduation_adapter != state.adapter(input.receiver)
                    || pool.ending_policy != state.ending_policy
                    || pool.trade_mode != state.trade_mode
                    || pool.protocol_fee_bps != state.protocol_fee_bps
                    || pool.sell_fee_bps != state.sell_fee_bps
                {
                    return Err(Error::InvalidCustody);
                }
                if pool.buy_policy != state.buy_policy
                    || pool.trade_control.policy() != state.pause_policy
                    || pool.funded
                    || pool.settlement_progress != SettlementProgress::Unselected
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
                let response = if state.creator_allocation_released {
                    state.allocation_event(
                        transfer(
                            response,
                            state.creator_escrow_id,
                            compute_escrow_seed(state.namespace, state.launch_salt),
                            state.token_definition_id,
                            creator,
                            state.creator_allocation,
                        ),
                        input.receiver.account_id,
                        creator,
                    )
                } else {
                    response
                };
                return state.with_setup_deadline(input.receiver, response);
            }
            if expected_stage == LbpCreationStage::Allocated {
                if balance(&pending.values[1], mint, state.token_definition_id)?
                    != state.graduation_reserve
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
                            protocol_fee_bps: state.protocol_fee_bps,
                            buy_policy: state.buy_policy.clone(),
                            pause_policy: state.pause_policy,
                            ending_policy: state.ending_policy,
                            trade_mode: state.trade_mode,
                            sell_fee_bps: state.sell_fee_bps,
                            graduation_adapter: state.adapter(input.receiver),
                            graduation_policy: state.graduation_policy,
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
                            protocol_fee_bps: state.protocol_fee_bps,
                            buy_policy: state.buy_policy.clone(),
                            pause_policy: state.pause_policy,
                            ending_policy: state.ending_policy,
                            trade_mode: state.trade_mode,
                            sell_fee_bps: state.sell_fee_bps,
                            graduation_adapter: state.adapter(input.receiver),
                            graduation_policy: state.graduation_policy,
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
                    input.receiver,
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
            return state.with_setup_deadline(
                input.receiver,
                transfer(
                    response,
                    mint,
                    compute_mint_seed(state.namespace, state.launch_salt),
                    state.token_definition_id,
                    state.creator_escrow_id,
                    state.creator_allocation,
                ),
            );
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
        if config.protocol_fee_bps >= 10_000 {
            return Err(Error::InvalidTerms);
        }
        state.protocol_fee_bps = config.protocol_fee_bps;
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
        let response = if matches!(state.mode, LbpSetupMode::Weighted { .. }) {
            let before = at_most.checked_add(1).ok_or(Error::InvalidTerms)?;
            curve_core::with_inclusion_bounds(response, None, Some(before))
                .map_err(|_| Error::InvalidTerms)?
        } else {
            match start {
                SaleStart::Now { timestamp } => {
                    curve_core::with_time_bounds(response, Some(timestamp), None)
                        .map_err(|_| Error::InvalidTerms)?
                }
                SaleStart::At { .. } => curve_core::with_inclusion_bounds(
                    response,
                    None,
                    Some(at_most.checked_add(1).ok_or(Error::InvalidTerms)?),
                )
                .map_err(|_| Error::InvalidTerms)?,
            }
        };
        if matches!(state.mode, LbpSetupMode::FixedPrice) {
            return state.with_setup_deadline(input.receiver, response);
        }
        state.with_setup_deadline(
            input.receiver,
            lez_compat_core::clock::guard(input.receiver, response, at_least, at_most)
                .map_err(|_| Error::InvalidTerms)?,
        )
    })())
}
impl LbpLaunchState {
    fn engine(&self) -> PricingEngineV1 {
        match self.mode {
            LbpSetupMode::FixedPrice => PricingEngineV1::FixedPrice,
            LbpSetupMode::Weighted { .. } => PricingEngineV1::Weighted,
        }
    }
    fn event(&self, factory: AccountId, kind: SaleEventKindV1) -> lee_core::program::ProgramEvent {
        launch_event(self.namespace, factory, self.pool_id, self.engine(), kind)
    }
    fn allocation_event(
        &self,
        response: Response,
        factory: AccountId,
        creator: AccountId,
    ) -> Response {
        if self.creator_allocation == 0 || creator == self.creator_escrow_id {
            return response;
        }
        response.event(self.event(
            factory,
            SaleEventKindV1::AllocationReleased {
                definition: *self.token_definition_id.value(),
                recipient: *creator.value(),
                amount: self.creator_allocation,
                release: match self.allocation_release {
                    CreatorAllocationRelease::AfterSetup => AllocationReleaseV1::AfterSetup,
                    CreatorAllocationRelease::AfterEnd => AllocationReleaseV1::AfterEnd,
                },
            },
        ))
    }
    fn validate_terms(&self) -> Result<(), Error> {
        if total_supply(
            self.sale_inventory,
            self.graduation_reserve,
            self.creator_allocation,
        )
        .map_err(|_| Error::InvalidState)?
            != self.total_supply
        {
            return Err(Error::InvalidState);
        }
        self.graduation_policy
            .validate(self.graduation_hook.is_some())
            .map_err(|_| Error::InvalidState)?;
        SellFeeRate::new(self.sell_fee_bps).map_err(|_| Error::InvalidState)?;
        validate_lbp_policies(
            self.mode,
            self.ending_policy,
            self.start_timestamp,
            self.end_timestamp,
        )
        .map_err(|_| Error::InvalidState)?;
        if self
            .graduation_hook
            .as_ref()
            .is_some_and(|hook| hook.program_id == AccountId::default())
        {
            return Err(Error::InvalidState);
        }
        let expected = match self.mode {
            LbpSetupMode::FixedPrice => *b"LZFXLAUN",
            LbpSetupMode::Weighted { .. } => *b"LZWGLAUN",
        };
        if self.format != expected || self.version != 6 || self.protocol_fee_bps >= 10_000 {
            return Err(Error::InvalidState);
        }
        Ok(())
    }
    fn with_setup_deadline(&self, receiver: Actor, response: Response) -> Result<Response, Error> {
        if matches!(self.mode, LbpSetupMode::FixedPrice) {
            return curve_core::with_time_bounds(response, None, self.end_timestamp)
                .map_err(|_| Error::InvalidTerms);
        }
        let Some(end_timestamp) = self.end_timestamp else {
            return Ok(response);
        };
        let response = if matches!(self.mode, LbpSetupMode::Weighted { .. }) {
            curve_core::with_inclusion_bounds(
                response,
                None,
                Some(curve_core::timestamp_millis(end_timestamp).map_err(|_| Error::InvalidTerms)?),
            )
            .map_err(|_| Error::InvalidTerms)?
        } else {
            response
        };
        let at_most = curve_core::timestamp_millis(end_timestamp)
            .map_err(|_| Error::InvalidTerms)?
            .checked_sub(1)
            .ok_or(Error::InvalidTerms)?;
        lez_compat_core::clock::guard(receiver, response, 0, at_most)
            .map_err(|_| Error::InvalidTerms)
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
        state.validate_terms()?;
        Ok(state)
    }
}

// Dynamic pricing collateral does not change the saved allocation terms.
fn payout_mode_matches(saved: LbpSetupMode, pool: lbp_core::PoolMode) -> bool {
    match (saved, pool) {
        (LbpSetupMode::FixedPrice, lbp_core::PoolMode::FixedPrice) => true,
        (
            LbpSetupMode::Weighted {
                collateral_seed: a,
                token_weight_start: b,
                token_weight_end: c,
            },
            lbp_core::PoolMode::Weighted {
                collateral_seed: x,
                token_weight_start: y,
                token_weight_end: z,
                ..
            },
        ) => (a, b, c) == (x, y, z),
        _ => false,
    }
}

impl LbpLaunchState {
    fn adapter(&self, factory: Actor) -> Option<Actor> {
        self.graduation_hook.as_ref().map(|hook| {
            Actor::new(
                compute_graduation_pda(hook.program_id, factory.account_id),
                hook.program_id,
            )
        })
    }

    fn validate_factory(&self, factory: Actor) -> Result<(), Error> {
        self.validate_terms()?;
        if factory.account_id
            != compute_factory_pda(self.namespace, factory.program_account_id, self.launch_salt)
            || self.token_definition_id
                != compute_definition_pda(
                    self.namespace,
                    factory.program_account_id,
                    self.launch_salt,
                )
            || self.creator_role_id
                != compute_creator_role_pda(
                    self.namespace,
                    factory.program_account_id,
                    self.launch_salt,
                )
            || self.creator_escrow_id
                != compute_escrow_pda(self.namespace, factory.program_account_id, self.launch_salt)
            || self.pool_id
                != lbp_core::compute_pool_pda(
                    self.namespace,
                    self.lbp_program_id,
                    self.token_definition_id,
                    self.collateral_definition_id,
                    factory.account_id,
                )
        {
            return Err(Error::WrongAccount);
        }
        Ok(())
    }

    fn validate_settlement_pool(
        &self,
        factory: Actor,
        pool: &lbp_core::PoolAccount,
    ) -> Result<(), Error> {
        self.validate_factory(factory)?;
        if pool.graduation_policy != self.graduation_policy
            || pool.graduation_adapter != self.adapter(factory)
            || pool.ending_policy != self.ending_policy
            || pool.trade_mode != self.trade_mode
            || pool.protocol_fee_bps != self.protocol_fee_bps
            || pool.sell_fee_bps != self.sell_fee_bps
        {
            return Err(Error::InvalidCustody);
        }
        if pool.buy_policy != self.buy_policy
            || pool.trade_control.policy() != self.pause_policy
            || !payout_mode_matches(self.mode, pool.mode)
            || pool.namespace != self.namespace
            || pool.namespace_program_id != self.namespace_program_id
            || pool.factory_owner != factory.account_id
            || pool.factory_program_id != factory.program_account_id
            || pool.factory_seed
                != *compute_factory_seed(self.namespace, self.launch_salt).as_bytes()
            || pool.token_definition_id != self.token_definition_id
            || pool.collateral_definition_id != self.collateral_definition_id
            || pool.sale_inventory != self.sale_inventory
            || pool.creator_allocation != self.creator_allocation
            || pool.price != self.price
            || pool.start_timestamp != self.start_timestamp
            || pool.end_timestamp != self.end_timestamp
            || pool.creator_allocation_release != self.allocation_release
            || pool.settlement_progress != self.settlement_progress
        {
            return Err(Error::InvalidState);
        }
        if !pool.funded {
            return Err(Error::CreationPending);
        }
        Ok(())
    }

    fn expected_receipt(
        &self,
        factory: Actor,
        pool: &lbp_core::PoolAccount,
        route: SettlementRoute,
        creator: AccountId,
    ) -> Result<SettlementReceipt, Error> {
        self.validate_settlement_pool(factory, pool)?;
        if creator == AccountId::default() {
            return Err(Error::Unauthorized);
        }
        let eligible = SettlementAmounts {
            launch_tokens: pool.remaining_inventory,
            collateral: pool.recorded_collateral,
        };
        let payouts = self
            .graduation_policy
            .payouts(eligible, route, self.graduation_hook.is_some())
            .map_err(|_| Error::InvalidTerms)?;
        let mut progress = self.settlement_progress;
        match route {
            SettlementRoute::Manual => {
                progress.record_manual_payout(eligible)?;
                progress.finish_manual()?;
            }
            SettlementRoute::Graduation => progress.finish_graduation()?,
        }
        Ok(SettlementReceipt {
            factory,
            pool: Actor::new(self.pool_id, self.lbp_program_id),
            route,
            creator,
            adapter: if route == SettlementRoute::Graduation {
                self.adapter(factory)
            } else {
                None
            },
            eligible,
            payouts,
            progress,
        })
    }

    fn start_pool_settlement(
        &mut self,
        factory: Actor,
        pool: &lbp_core::PoolAccount,
        route: SettlementRoute,
        creator: AccountId,
    ) -> Result<Response, Error> {
        let receipt = self.expected_receipt(factory, pool, route, creator)?;
        self.settlement_pending = Some(LbpSettlementPhase::PoolApplied {
            receipt: receipt.clone(),
        });
        let response = self
            .response()
            .send_sync(
                Actor::new(self.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Initialize { owner: creator },
            )
            .send_sync(
                Actor::new(self.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                &token::Message::Initialize { owner: creator },
            )
            .send_sync(
                receipt.pool,
                &lbp_core::Message::Settle {
                    route,
                    creator,
                    adapter: receipt.adapter,
                },
            );
        curve_core::with_time_bounds(
            response,
            pool.close_time_bound().map_err(|e| match e {
                lbp_core::Error::NotClosed => Error::NotClosed,
                _ => Error::InvalidTerms,
            })?,
            None,
        )
        .map_err(|_| Error::InvalidTerms)
    }

    fn graduation_request(
        &self,
        factory: Actor,
        receipt: &SettlementReceipt,
    ) -> Result<GraduationRequest, Error> {
        if receipt.route != SettlementRoute::Graduation
            || receipt.factory != factory
            || receipt.pool != Actor::new(self.pool_id, self.lbp_program_id)
            || receipt.adapter != self.adapter(factory)
        {
            return Err(Error::InvalidState);
        }
        let hook = self
            .graduation_hook
            .as_ref()
            .ok_or(Error::HookNotConfigured)?;
        let intent = match self.mode {
            LbpSetupMode::FixedPrice => GraduationIntent::FixedPrice {
                launch_tokens: receipt
                    .payouts
                    .adapter
                    .launch_tokens
                    .checked_add(self.graduation_reserve)
                    .ok_or(Error::Overflow)?,
                collateral: receipt.payouts.adapter.collateral,
            },
            LbpSetupMode::Weighted { .. } => GraduationIntent::Weighted {
                launch_tokens: receipt
                    .payouts
                    .adapter
                    .launch_tokens
                    .checked_add(self.graduation_reserve)
                    .ok_or(Error::Overflow)?,
                collateral: receipt.payouts.adapter.collateral,
            },
        };
        Ok(GraduationRequest {
            factory_program_id: factory.program_account_id,
            namespace: self.namespace,
            launch_salt: self.launch_salt,
            token_definition_id: self.token_definition_id,
            collateral_definition_id: self.collateral_definition_id,
            intent,
            reserve_tokens: self.graduation_reserve,
            unsold_tokens: receipt.payouts.adapter.launch_tokens,
            configuration: hook.configuration.clone(),
        })
    }
}

pub(super) fn begin_hook(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
    let mut state = LbpLaunchState::decode(&input.pre_state)?;
    state.validate_factory(input.receiver)?;
    if state.creation_stage != LbpCreationStage::Complete {
        return Err(Error::CreationPending);
    }
    let adapter = state
        .adapter(input.receiver)
        .ok_or(Error::HookNotConfigured)?;
    state
        .settlement_progress
        .ensure_available(SettlementRoute::Graduation)?;
    let Message::ExecuteGraduationHook {
        creator_hint: Some(creator_hint),
    } = command
    else {
        return Err(Error::InvalidTerms);
    };
    if creator_hint == AccountId::default() {
        return Err(Error::InvalidTerms);
    }
    validate_adapter_custody(&state, input.receiver, adapter)?;
    state.pending = Some(Pending {
        command,
        proof: false,
        reads: vec![
            Read::new(
                Actor::new(state.pool_id, state.lbp_program_id),
                &lbp_core::Message::ReadState,
            ),
            Read::new(adapter, &AdapterMessage::ReadState),
        ],
        values: vec![],
    });
    Ok(state.next())
}

fn validate_adapter_custody(
    state: &LbpLaunchState,
    factory: Actor,
    adapter: Actor,
) -> Result<(), Error> {
    if adapter.program_account_id == AccountId::default()
        || adapter.account_id == factory.account_id
        || adapter.account_id == state.pool_id
        || adapter.account_id == state.creator_escrow_id
        || adapter.account_id == state.token_definition_id
        || adapter.account_id == state.collateral_definition_id
        || adapter.account_id == state.creator_role_id
        || adapter.account_id
            == compute_mint_pda(
                state.namespace,
                factory.program_account_id,
                state.launch_salt,
            )
    {
        return Err(Error::InvalidCustody);
    }
    Ok(())
}

fn settlement_ready(
    input: &ReceiveInput,
    mut state: LbpLaunchState,
    pending: Pending,
) -> Result<Response, Error> {
    let pool =
        lbp_core::PoolAccount::decode(&pending.values[0]).map_err(|_| Error::InvalidState)?;
    state.validate_settlement_pool(input.receiver, &pool)?;
    let expiry = pool.close_time_bound().map_err(|error| match error {
        lbp_core::Error::NotClosed => Error::NotClosed,
        _ => Error::InvalidTerms,
    })?;
    match pending.command {
        Message::WithdrawFactoryProceeds { creator, .. }
        | Message::WithdrawWeightedProceeds { creator } => {
            state
                .graduation_policy
                .validate_route(SettlementRoute::Manual, state.graduation_hook.is_some())
                .map_err(|_| Error::InvalidTerms)?;
            if state.settlement_progress.is_complete() {
                return curve_core::with_time_bounds(state.response(), expiry, None)
                    .map_err(|_| Error::InvalidTerms);
            }
            state.start_pool_settlement(input.receiver, &pool, SettlementRoute::Manual, creator)
        }
        Message::ExecuteGraduationHook {
            creator_hint: Some(creator_hint),
        } => {
            state
                .settlement_progress
                .ensure_available(SettlementRoute::Graduation)?;
            let adapter_actor = state
                .adapter(input.receiver)
                .ok_or(Error::HookNotConfigured)?;
            validate_adapter_custody(&state, input.receiver, adapter_actor)?;
            let adapter = AdapterState::decode(&pending.values[1])?;
            if adapter.factory != input.receiver || adapter.receipt.is_some() {
                return Err(Error::InvalidCustody);
            }
            let kind = match state.mode {
                LbpSetupMode::FixedPrice => GraduationIntentKind::FixedPrice,
                LbpSetupMode::Weighted { .. } => GraduationIntentKind::Weighted,
            };
            if adapter.accepted_intent != kind {
                return Err(Error::AdapterIntentMismatch);
            }
            state.settlement_pending = Some(LbpSettlementPhase::AdapterHoldings {
                pool,
                creator_hint,
                step: 0,
            });
            let read = holding_read(adapter_actor.account_id, state.token_definition_id);
            let mut call = SyncMessage::new(read.actor, &());
            call.message = read.message;
            let response = state
                .response()
                .send_sync(
                    Actor::new(state.token_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::Initialize {
                        owner: adapter_actor.account_id,
                    },
                )
                .send_sync(
                    Actor::new(state.collateral_definition_id, TOKEN_PROGRAM_ACCOUNT_ID),
                    &token::Message::Initialize {
                        owner: adapter_actor.account_id,
                    },
                )
                .send(call);
            curve_core::with_time_bounds(response, expiry, None).map_err(|_| Error::InvalidTerms)
        }
        _ => Err(Error::InvalidState),
    }
}

fn resume_settlement(input: &ReceiveInput, mut state: LbpLaunchState) -> Result<Response, Error> {
    state.validate_factory(input.receiver)?;
    if state.pending.is_some() {
        return Err(Error::InvalidState);
    }
    let phase = state.settlement_pending.take().ok_or(Error::InvalidState)?;
    match phase {
        LbpSettlementPhase::AdapterHoldings {
            pool,
            creator_hint,
            step,
        } => {
            if step > 1 {
                return Err(Error::InvalidState);
            }
            let adapter = state
                .adapter(input.receiver)
                .ok_or(Error::HookNotConfigured)?;
            let definition = if step == 0 {
                state.token_definition_id
            } else {
                state.collateral_definition_id
            };
            let read = holding_read(adapter.account_id, definition);
            if input.from != Some(read.actor) {
                return Err(Error::Unauthorized);
            }
            let reply: StateReply =
                borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
            balance(&reply.state, adapter.account_id, definition)?;
            if step == 0 {
                state.settlement_pending = Some(LbpSettlementPhase::AdapterHoldings {
                    pool,
                    creator_hint,
                    step: 1,
                });
                let read = holding_read(adapter.account_id, state.collateral_definition_id);
                let mut call = SyncMessage::new(read.actor, &());
                call.message = read.message;
                Ok(state.response().send(call))
            } else {
                state.settlement_pending = Some(LbpSettlementPhase::Creator { pool, creator_hint });
                Ok(state.response().send_sync(
                    Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
                    &authority::Message::Resolve,
                ))
            }
        }
        LbpSettlementPhase::Creator { pool, creator_hint } => {
            let creator = authority::verify_resolved(
                input,
                Actor::new(state.creator_role_id, AUTHORITY_PROGRAM_ACCOUNT_ID),
            )?;
            if creator != creator_hint {
                return Err(Error::Unauthorized);
            }
            if state
                .adapter(input.receiver)
                .is_some_and(|adapter| adapter.account_id == creator)
            {
                return Err(Error::InvalidCustody);
            }
            state.start_pool_settlement(input.receiver, &pool, SettlementRoute::Graduation, creator)
        }
        LbpSettlementPhase::PoolApplied { receipt: expected } => {
            if input.from != Some(expected.pool) {
                return Err(Error::Unauthorized);
            }
            let SettlementReply::Applied { receipt } =
                borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?
            else {
                return Err(Error::InvalidMessage);
            };
            if receipt != expected {
                return Err(Error::InvalidState);
            }
            let recipient = receipt
                .adapter
                .map_or(receipt.creator, |adapter| adapter.account_id);
            let mut reserve_effects = vec![];
            let response = reserve_payout(
                Response::keep_state(),
                input.receiver,
                state.namespace,
                state.launch_salt,
                state.token_definition_id,
                recipient,
                state.graduation_reserve,
                &mut reserve_effects,
            );
            let reserve_messages = response.sync_messages;
            let reserve_events = if reserve_effects.is_empty() {
                vec![]
            } else {
                vec![state.event(
                    input.receiver.account_id,
                    SaleEventKindV1::Settlement {
                        step: SettlementStepV1::Payout,
                        route: Some(match receipt.route {
                            SettlementRoute::Manual => SettlementRouteV1::Manual,
                            SettlementRoute::Graduation => SettlementRouteV1::Graduation,
                        }),
                        effects: reserve_effects,
                    },
                )]
            };
            let with_reserve = |mut response: Response| {
                response.sync_messages.splice(0..0, reserve_messages);
                response.events.extend(reserve_events);
                response
            };
            if receipt.route == SettlementRoute::Manual {
                let receipt_hash = receipt.hash();
                state.settlement_pending = Some(LbpSettlementPhase::PoolCompleted {
                    receipt_hash,
                    progress: receipt.progress,
                    graduation_amounts: None,
                });
                Ok(with_reserve(state.response().send_sync(
                    receipt.pool,
                    &lbp_core::Message::FinalizeSettlement { receipt_hash },
                )))
            } else {
                let request = state.graduation_request(input.receiver, &receipt)?;
                let adapter = receipt.adapter.ok_or(Error::InvalidState)?;
                state.settlement_pending = Some(LbpSettlementPhase::AdapterResult {
                    receipt,
                    request_hash: request.hash(),
                });
                Ok(with_reserve(
                    state
                        .response()
                        .send_sync(adapter, &AdapterMessage::Graduate { request }),
                ))
            }
        }
        LbpSettlementPhase::AdapterResult {
            receipt,
            request_hash,
        } => {
            if input.from != receipt.adapter {
                return Err(Error::Unauthorized);
            }
            let result: GraduationResult =
                borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?;
            if result.factory != input.receiver.account_id || result.request_hash != request_hash {
                return Err(Error::InvalidState);
            }
            let receipt_hash = receipt.hash();
            state.settlement_pending = Some(LbpSettlementPhase::PoolCompleted {
                receipt_hash,
                progress: receipt.progress,
                graduation_amounts: Some(SettlementAmounts {
                    launch_tokens: receipt
                        .payouts
                        .adapter
                        .launch_tokens
                        .checked_add(state.graduation_reserve)
                        .ok_or(Error::Overflow)?,
                    collateral: receipt.payouts.adapter.collateral,
                }),
            });
            Ok(state.response().send_sync(
                receipt.pool,
                &lbp_core::Message::FinalizeSettlement { receipt_hash },
            ))
        }
        LbpSettlementPhase::PoolCompleted {
            receipt_hash,
            progress,
            graduation_amounts,
        } => {
            if input.from != Some(Actor::new(state.pool_id, state.lbp_program_id)) {
                return Err(Error::Unauthorized);
            }
            let SettlementReply::Completed {
                receipt_hash: received_hash,
                progress: received_progress,
            } = borsh::from_slice(&input.message).map_err(|_| Error::InvalidMessage)?
            else {
                return Err(Error::InvalidMessage);
            };
            if received_hash != receipt_hash || received_progress != progress {
                return Err(Error::InvalidState);
            }
            state.settlement_progress = progress;
            let response = state.response();
            Ok(if let Some(amounts) = graduation_amounts {
                let adapter = state
                    .adapter(input.receiver)
                    .ok_or(Error::HookNotConfigured)?;
                response.event(state.event(
                    input.receiver.account_id,
                    SaleEventKindV1::Graduated {
                        adapter_program: *adapter.program_account_id.value(),
                        adapter_custody: *adapter.account_id.value(),
                        launch_tokens: amounts.launch_tokens,
                        collateral: amounts.collateral,
                    },
                ))
            } else {
                response
            })
        }
    }
}
