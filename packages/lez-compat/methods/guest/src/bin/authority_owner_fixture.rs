//! Signed controller and PDA successor fixture for guest authority tests.
use lee_core::{
    account::{AccountId, Actor},
    program::{PdaSeed, ReceiveInput, Response, SyncMessage},
};

lee_core::actor_entrypoint!(raw handle);

const AUTHORITY: AccountId = AccountId::new([
    94, 144, 155, 106, 14, 146, 62, 120, 135, 16, 55, 144, 76, 89, 251, 81, 15, 154, 235, 51, 72,
    22, 124, 218, 6, 52, 198, 24, 253, 158, 192, 243,
]);

fn handle(input: &ReceiveInput) -> Response {
    if input.from.is_some() {
        return lez_compat_core::authority::approve_pda(input, AUTHORITY)
            .expect("successor consent rejected");
    }
    assert!(input.is_authorized, "owner controller must sign");
    assert!(input.message.len() > 32, "missing forwarded instruction");
    let program = AccountId::new(input.message[..32].try_into().unwrap());
    let owner_program = input.receiver.program_account_id;
    let seed = PdaSeed::new(*input.receiver.account_id.value());
    let successor = AccountId::for_public_pda(&owner_program, &seed);
    let mut call = SyncMessage::new(Actor::new(successor, program), &());
    call.message = input.message[32..].to_vec();
    Response::keep_state().send(call.with_pda_seeds([seed]))
}
