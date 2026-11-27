//! Shared token, allocation and authority setup for typed LBP sale inputs.
use super::*;
pub use lbp_core::{CreatorAllocationRelease, FixedPrice};

mod funding;
mod post_close;
pub(super) use post_close::{begin_hook, begin_payout};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize,
)]
pub enum SaleStart {
    Now { timestamp: u64 },
    At { timestamp: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct FixedPriceInputs {
    pub per_block_token_limit: Option<u128>,
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
    pub per_block_token_limit: Option<u128>,
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
    pub per_block_token_limit: Option<u128>,
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
    per_block_token_limit: Option<u128>,
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
    let (creator, now) = (inputs.creator, inputs.now);
    let mut response = create(
        input,
        SetupInputs {
            per_block_token_limit: inputs.per_block_token_limit,
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
    )?;
    for expected_stage in [
        LbpCreationStage::Minted,
        LbpCreationStage::Allocated,
        LbpCreationStage::PoolPrepared,
    ] {
        response = response.send_sync(
            input.receiver,
            &Message::ContinueFixedPriceCreation {
                expected_stage,
                creator,
                now,
            },
        );
    }
    Ok(response)
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
    let creator = inputs.creator;
    let mut response = create(
        input,
        SetupInputs {
            per_block_token_limit: inputs.per_block_token_limit,
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
    )?;
    for expected_stage in [
        LbpCreationStage::Minted,
        LbpCreationStage::Allocated,
        LbpCreationStage::PoolPrepared,
    ] {
        response = response.send_sync(
            input.receiver,
            &Message::ContinueWeightedCreation {
                expected_stage,
                creator,
            },
        );
    }
    Ok(response)
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
    if inputs.per_block_token_limit == Some(0) {
        return Err(Error::InvalidTerms);
    }
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
        per_block_token_limit: inputs.per_block_token_limit,
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
        version: 8,
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

pub(super) fn resume(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    let mut state: LbpLaunchState = borsh::from_slice(&input.pre_state).ok()?;
    if state.settlement_pending.is_some() {
        return Some(post_close::resume_settlement(input, state));
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
            if matches!(
                pending.command,
                Message::SetPaused { .. }
                    | Message::SetWeightedPaused { .. }
                    | Message::WithdrawWeightedProceeds { .. }
                    | Message::WithdrawFactoryProceeds { .. }
                    | Message::ClaimCreatorAllocation { .. }
                    | Message::ClaimWeightedAllocation { .. }
            ) {
                return post_close::resume_authorized(state, pending);
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
            return post_close::after_graduation_pool_read(input, state, pending);
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
            return post_close::settlement_ready(input, state, pending);
        }
        if let Message::ClaimCreatorAllocation { creator, .. }
        | Message::ClaimWeightedAllocation { creator } = pending.command
        {
            return post_close::finish_allocation(input, state, creator, &pending.values);
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
            return finish_stage(input, state, expected_stage, creator, &pending.values);
        }
        finish_mint(input, state, pending)
    })())
}

fn finish_stage(
    input: &ReceiveInput,
    mut state: LbpLaunchState,
    expected_stage: LbpCreationStage,
    creator: AccountId,
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
    if expected_stage == LbpCreationStage::PoolPrepared {
        return funding::complete(input.receiver, state, mint, creator, values);
    }
    if expected_stage == LbpCreationStage::Allocated {
        return funding::prepare(input.receiver, state, mint, values);
    }
    if expected_stage != LbpCreationStage::Minted {
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
    state.with_setup_deadline(
        input.receiver,
        transfer(
            response,
            mint,
            compute_mint_seed(state.namespace, state.launch_salt),
            state.token_definition_id,
            state.creator_escrow_id,
            state.creator_allocation,
        ),
    )
}

fn finish_mint(
    input: &ReceiveInput,
    mut state: LbpLaunchState,
    pending: Pending,
) -> Result<Response, Error> {
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
    let config = curve_core::Config::decode(&pending.values[0]).map_err(|_| Error::InvalidState)?;
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
        if self.per_block_token_limit == Some(0) {
            return Err(Error::InvalidState);
        }
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
        if self.format != expected || self.version != 8 {
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

    fn adapter(&self, factory: Actor) -> Option<Actor> {
        self.graduation_hook.as_ref().map(|hook| {
            Actor::new(
                compute_graduation_pda(hook.program_id, factory.account_id),
                hook.program_id,
            )
        })
    }
}
