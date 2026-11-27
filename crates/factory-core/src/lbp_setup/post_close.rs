//! LBP controls, creator allocation, manual settlement and graduation.
use super::*;

pub(crate) fn begin_payout(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
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

pub(super) fn resume_authorized(
    mut state: LbpLaunchState,
    mut pending: Pending,
) -> Result<Response, Error> {
    match pending.command {
        Message::SetPaused { paused, .. } => Ok(state.response().send(
            SyncMessage::new(
                Actor::new(state.pool_id, state.lbp_program_id),
                &lbp_core::Message::SetPaused { paused },
            )
            .with_pda_seeds([compute_factory_seed(state.namespace, state.launch_salt)]),
        )),
        Message::SetWeightedPaused { paused, .. } => Ok(state.response().send_sync(
            Actor::new(state.pool_id, state.lbp_program_id),
            &lbp_core::Message::SetWeightedPaused { paused },
        )),
        Message::ClaimCreatorAllocation { .. } | Message::ClaimWeightedAllocation { .. }
            if state.creator_allocation_released
                && state.allocation_release == CreatorAllocationRelease::AfterSetup =>
        {
            Ok(state.response())
        }
        _ => {
            pending.proof = false;
            state.pending = Some(pending);
            Ok(state.next())
        }
    }
}

pub(super) fn finish_allocation(
    input: &ReceiveInput,
    mut state: LbpLaunchState,
    creator: AccountId,
    values: &[Vec<u8>],
) -> Result<Response, Error> {
    let pool = lbp_core::PoolAccount::decode(&values[0]).map_err(|_| Error::InvalidState)?;
    if pool.graduation_policy != state.graduation_policy
        || pool.graduation_adapter != state.adapter(input.receiver)
        || pool.ending_policy != state.ending_policy
        || pool.trade_mode != state.trade_mode
        || pool.block_allocation.limit != state.per_block_token_limit
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
        || pool.factory_seed != *compute_factory_seed(state.namespace, state.launch_salt).as_bytes()
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
            &values[1],
            state.creator_escrow_id,
            state.token_definition_id,
        )? < state.creator_allocation
    {
        return Err(Error::InvalidCustody);
    }
    state.creator_allocation_released = true;
    let response = curve_core::with_time_bounds(state.response(), expiry, None)
        .map_err(|_| Error::InvalidTerms)?;
    Ok(state.allocation_event(
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
    ))
}

pub(super) fn after_graduation_pool_read(
    input: &ReceiveInput,
    mut state: LbpLaunchState,
    pending: Pending,
) -> Result<Response, Error> {
    let pool =
        lbp_core::PoolAccount::decode(&pending.values[0]).map_err(|_| Error::InvalidState)?;
    state.validate_settlement_pool(input.receiver, &pool)?;
    state
        .settlement_progress
        .ensure_available(SettlementRoute::Graduation)?;
    let expiry = pool.close_time_bound().map_err(|error| match error {
        lbp_core::Error::NotClosed => Error::NotClosed,
        _ => Error::InvalidTerms,
    })?;
    state.pending = Some(pending);
    curve_core::with_time_bounds(state.next(), expiry, None).map_err(|_| Error::InvalidTerms)
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
            || pool.block_allocation.limit != self.per_block_token_limit
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

pub(crate) fn begin_hook(input: &ReceiveInput, command: Message) -> Result<Response, Error> {
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

pub(super) fn settlement_ready(
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

pub(super) fn resume_settlement(
    input: &ReceiveInput,
    mut state: LbpLaunchState,
) -> Result<Response, Error> {
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
