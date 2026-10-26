use curve_core::{Config, INITIAL_EMERGENCY_AUTHORITY, Message, compute_config_pda, handle};
use lee_core::{
    account::{AccountId, Actor, ActorState},
    program::ReceiveInput,
};
const PROGRAM: AccountId = AccountId::new([42; 32]);
const NAMESPACE: AccountId = AccountId::new([1; 32]);
fn invoke(
    authority: AccountId,
    state: ActorState,
    message: Message,
) -> Result<ActorState, curve_core::Error> {
    let request = ReceiveInput {
        receiver: Actor::new(authority, PROGRAM),
        from: None,
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: borsh::to_vec(&message).unwrap(),
    };
    let call = handle(&request)?.sync_messages.remove(0);
    let result = handle(&ReceiveInput {
        receiver: call.to,
        from: Some(request.receiver),
        is_authorized: false,
        pre_state: state,
        message: call.message,
    })?;
    Ok(result.post_state.unwrap())
}
fn initialize() -> ActorState {
    invoke(
        NAMESPACE,
        ActorState::empty(),
        Message::InitializeNamespace {
            namespace: NAMESPACE,
            protocol_fee_bps: 500,
            treasury: AccountId::new([6; 32]),
        },
    )
    .unwrap()
}
#[test]
fn only_the_deployment_emergency_holder_can_freeze_or_repeat_freeze() {
    let original = initialize();
    let mut state = original.clone();
    for frozen in [true, true, false, false] {
        state = invoke(
            INITIAL_EMERGENCY_AUTHORITY,
            state,
            Message::SetFrozen {
                namespace: NAMESPACE,
                frozen,
            },
        )
        .unwrap();
        let decoded = Config::decode(&state).unwrap();
        assert_eq!(decoded.emergency.is_frozen, frozen);
        assert_eq!(decoded.protocol_fee_bps, 500);
        assert!(
            invoke(
                NAMESPACE,
                state.clone(),
                Message::SetFrozen {
                    namespace: NAMESPACE,
                    frozen
                }
            )
            .is_err()
        );
    }
    let unsigned = ReceiveInput {
        receiver: Actor::new(INITIAL_EMERGENCY_AUTHORITY, PROGRAM),
        from: None,
        is_authorized: false,
        pre_state: ActorState::empty(),
        message: borsh::to_vec(&Message::SetFrozen {
            namespace: NAMESPACE,
            frozen: false,
        })
        .unwrap(),
    };
    assert!(handle(&unsigned).is_err());
    assert_ne!(
        compute_config_pda(NAMESPACE, PROGRAM),
        compute_config_pda(AccountId::new([2; 32]), PROGRAM)
    );
}

#[test]
fn emergency_transfer_is_immediate_keeps_freeze_and_cannot_renounce() {
    let frozen = invoke(
        INITIAL_EMERGENCY_AUTHORITY,
        initialize(),
        Message::SetFrozen {
            namespace: NAMESPACE,
            frozen: true,
        },
    )
    .unwrap();
    let successor = AccountId::new([102; 32]);
    let transferred = invoke(
        INITIAL_EMERGENCY_AUTHORITY,
        frozen.clone(),
        Message::TransferEmergencyAuthority {
            namespace: NAMESPACE,
            new_authority: successor,
        },
    )
    .unwrap();
    assert_eq!(
        Config::decode(&transferred).unwrap().emergency.authority(),
        successor
    );
    assert!(Config::decode(&transferred).unwrap().emergency.is_frozen);
    assert!(
        invoke(
            INITIAL_EMERGENCY_AUTHORITY,
            transferred.clone(),
            Message::SetFrozen {
                namespace: NAMESPACE,
                frozen: true
            }
        )
        .is_err()
    );
    assert!(
        invoke(
            successor,
            transferred.clone(),
            Message::TransferEmergencyAuthority {
                namespace: NAMESPACE,
                new_authority: AccountId::default()
            }
        )
        .is_err()
    );
    let same = invoke(
        successor,
        transferred.clone(),
        Message::TransferEmergencyAuthority {
            namespace: NAMESPACE,
            new_authority: successor,
        },
    )
    .unwrap();
    assert_eq!(same, transferred);
}
