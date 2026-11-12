//! Weighted controls, scheduled closing and observer checkpoints.
//! Fixed-price sellout rules do not apply.
use crate::{Error, PoolAccount, PoolMode, WEIGHT_TOTAL, valid_weight_endpoints};
use borsh::{BorshDeserialize, BorshSerialize};
use lee_core::{
    account::Actor,
    program::{ProgramEvent, ReceiveInput, Response},
};
use weighted_math_core::interpolate_weight;

pub(super) fn end(state: &PoolAccount) -> Result<u64, Error> {
    if !matches!(state.mode, PoolMode::Weighted { .. }) {
        return Err(Error::InvalidMessage);
    }
    state.end_timestamp.ok_or(Error::InvalidState)
}
pub(super) fn authorize(state: &PoolAccount, input: &ReceiveInput) -> Result<(), Error> {
    end(state)?;
    state.validate_address(input.receiver)?;
    if !state.funded {
        return Err(Error::PendingFunding);
    }
    if input.from != Some(Actor::new(state.factory_owner, state.factory_program_id)) {
        return Err(Error::Unauthorized);
    }
    Ok(())
}
pub(super) fn set_paused(input: &ReceiveInput, paused: bool) -> Result<Response, Error> {
    let mut state = PoolAccount::decode(&input.pre_state)?;
    authorize(&state, input)?;
    let end = end(&state)?;
    if state.proceeds_settled {
        return Err(Error::Closed);
    }
    let PoolMode::Weighted { paused: stored, .. } = &mut state.mode else {
        unreachable!()
    };
    let changed = *stored != paused;
    *stored = paused;
    let response = curve_core::with_time_bounds(state.response(), None, Some(end))
        .map_err(|_| Error::InvalidTerms)?;
    Ok(if changed {
        response.event(ProgramEvent {
            // First eight bytes of sha256("lbp::WeightedPauseChanged").
            selector: [109, 140, 24, 3, 232, 43, 58, 157],
            data: borsh::to_vec(&paused).unwrap(),
        })
    } else {
        response
    })
}

// ---------------------------------------------------------------------------
// Permissionless poke — observer checkpoint
// ---------------------------------------------------------------------------

const POKE_TAG: [u8; 8] = *b"LZLBPOKE";

#[derive(BorshSerialize, BorshDeserialize)]
struct PokePending {
    tag: [u8; 8],
    state: PoolAccount,
}

/// Compute collateral and token weights with one interpolation.
pub(super) fn weights_at(state: &PoolAccount, time: u64) -> Result<(u128, u128), Error> {
    let PoolMode::Weighted {
        token_weight_start,
        token_weight_end,
        ..
    } = state.mode
    else {
        return Err(Error::InvalidMessage);
    };
    if !valid_weight_endpoints(token_weight_start, token_weight_end) {
        return Err(Error::InvalidTerms);
    }
    let end_ts = state.end_timestamp.ok_or(Error::InvalidState)?;
    let collateral_w = interpolate_weight(
        WEIGHT_TOTAL - token_weight_start,
        WEIGHT_TOTAL - token_weight_end,
        state.start_timestamp,
        end_ts,
        time,
    );
    Ok((collateral_w, WEIGHT_TOTAL - collateral_w))
}

/// Record an observed weight and return an event only when the checkpoint changes.
pub(super) fn record_checkpoint(
    state: &mut PoolAccount,
    time: u64,
    token_w: u128,
) -> Option<ProgramEvent> {
    let PoolMode::Weighted {
        checkpoint_time,
        checkpoint_token_weight,
        ..
    } = &mut state.mode
    else {
        unreachable!()
    };
    if *checkpoint_time == time && *checkpoint_token_weight == token_w {
        return None;
    }
    *checkpoint_time = time;
    *checkpoint_token_weight = token_w;
    Some(checkpoint_event(time, token_w))
}

/// Checkpoint event selector: first eight bytes of sha256("lbp::WeightedCheckpoint").
const CHECKPOINT_SELECTOR: [u8; 8] = [0xae, 0x2f, 0x6d, 0x45, 0x7f, 0xb3, 0x79, 0xcf];

fn checkpoint_event(time: u64, token_weight: u128) -> ProgramEvent {
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&time.to_le_bytes());
    data.extend_from_slice(&token_weight.to_le_bytes());
    ProgramEvent {
        selector: CHECKPOINT_SELECTOR,
        data,
    }
}

/// Begin a permissionless poke: validate pool, then request a clock read.
pub(super) fn poke(input: &ReceiveInput) -> Result<Response, Error> {
    let state = PoolAccount::decode(&input.pre_state)?;
    state.validate_address(input.receiver)?;
    if !matches!(state.mode, PoolMode::Weighted { .. }) {
        return Err(Error::InvalidMessage);
    }
    if !state.funded {
        return Err(Error::PendingFunding);
    }
    let pending = PokePending {
        tag: POKE_TAG,
        state,
    };
    Ok(
        Response::set_state(borsh::to_vec(&pending).unwrap()).send_sync(
            crate::pricing_clock::actor(),
            &crate::pricing_clock::Request::ReadTimestamp,
        ),
    )
}

/// Resume a pending poke after the clock reply.
pub(super) fn resume_poke(input: &ReceiveInput) -> Option<Result<Response, Error>> {
    if !input.pre_state.starts_with(&POKE_TAG) {
        return None;
    }
    Some((|| {
        let pending: PokePending =
            borsh::from_slice(&input.pre_state).map_err(|_| Error::InvalidState)?;
        let mut state = pending.state;
        state.validate_address(input.receiver)?;
        let time = crate::pricing_clock::read(input)?;
        let (_, token_weight) = weights_at(&state, time)?;
        let event = record_checkpoint(&mut state, time, token_weight);
        let response = state.response();
        Ok(if let Some(event) = event {
            response.event(event)
        } else {
            response
        })
    })())
}
