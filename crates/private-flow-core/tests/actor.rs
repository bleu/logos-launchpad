use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::ReceiveInput,
};
use private_flow_core::{Error, handle_private_authority, handle_private_buy};

#[test]
fn private_purchase_is_rejected_until_private_token_returns_are_supported() {
    let input = ReceiveInput {
        receiver: Actor::new(AccountId::new([1; 32]), AccountId::new([2; 32])),
        from: None,
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: Vec::new(),
    };
    assert!(matches!(
        handle_private_buy(&input),
        Err(Error::PrivatePurchaseUnsupported)
    ));
}

#[test]
fn private_authority_handoff_is_rejected_without_private_holdings_support() {
    let input = ReceiveInput {
        receiver: Actor::new(AccountId::new([1; 32]), AccountId::new([2; 32])),
        from: None,
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: Vec::new(),
    };
    assert!(matches!(
        handle_private_authority(&input),
        Err(Error::PrivateAuthorityUnsupported)
    ));
}
