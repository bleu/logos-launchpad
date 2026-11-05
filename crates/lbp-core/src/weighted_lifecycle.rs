//! Weighted controls and scheduled closing. Fixed-price sellout rules do not apply.
use crate::{Error, PoolAccount, PoolMode};
use lee_core::{
    account::Actor,
    program::{ProgramEvent, ReceiveInput, Response},
};

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
