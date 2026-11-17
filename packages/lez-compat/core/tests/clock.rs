use lee_core::{
    account::{AccountId, Actor},
    program::{ReceiveInput, Response},
};
use lez_compat_core::{Error, clock};
fn receiver() -> Actor {
    Actor::new(AccountId::new([31; 32]), AccountId::new([32; 32]))
}
fn response() -> Response {
    let mut response = Response::set_state(vec![9, 8, 7]).send_sync(receiver(), &42u64);
    response.validity.timestamps = (Some(100), Some(201)).try_into().unwrap();
    response
}
fn pending() -> Response {
    clock::guard(receiver(), response(), 100, 200).unwrap()
}
fn reply(pending: Response, timestamp: u64) -> ReceiveInput {
    ReceiveInput {
        receiver: receiver(),
        from: Some(clock_core::clock_actor()),
        is_authorized: false,
        pre_state: pending.post_state.unwrap(),
        message: borsh::to_vec(&clock_core::Reply::Timestamp(timestamp)).unwrap(),
    }
}
#[test]
fn guard_defers_all_effects_and_restores_the_exact_response_at_inclusive_bounds() {
    for timestamp in [100, 150, 200] {
        let pending = pending();
        assert_eq!(pending.sync_messages.len(), 1);
        assert_eq!(pending.sync_messages[0].to, clock_core::clock_actor());
        assert_eq!(
            pending.sync_messages[0].message,
            borsh::to_vec(&clock::request()).unwrap()
        );
        let actual = clock::resume(&reply(pending, timestamp)).unwrap().unwrap();
        assert_eq!(
            borsh::to_vec(&actual).unwrap(),
            borsh::to_vec(&response()).unwrap()
        );
    }
}
#[test]
fn guard_rejects_forged_senders_wrong_receivers_bad_replies_and_outside_times() {
    for from in [
        None,
        Some(Actor::new(
            AccountId::new([1; 32]),
            clock_core::clock_account_id(),
        )),
        Some(Actor::new(
            clock_core::CLOCK_PROGRAM_ACCOUNT_ID,
            AccountId::new([2; 32]),
        )),
    ] {
        let mut input = reply(pending(), 150);
        input.from = from;
        assert_eq!(
            clock::resume(&input).unwrap().err(),
            Some(Error::Unauthorized)
        );
    }
    let mut input = reply(pending(), 150);
    input.receiver.account_id = AccountId::new([3; 32]);
    assert_eq!(
        clock::resume(&input).unwrap().err(),
        Some(Error::WrongAccount)
    );
    for bytes in [
        vec![],
        vec![255],
        borsh::to_vec(&clock_core::Reply::BlockId(150)).unwrap(),
    ] {
        let mut input = reply(pending(), 150);
        input.message = bytes;
        assert_eq!(
            clock::resume(&input).unwrap().err(),
            Some(Error::InvalidMessage)
        );
    }
    for timestamp in [99, 201] {
        assert_eq!(
            clock::resume(&reply(pending(), timestamp)).unwrap().err(),
            Some(Error::OutsideTimeBounds)
        );
    }
    let mut input = reply(pending(), 150);
    input.pre_state = vec![9, 8, 7].into();
    assert!(clock::resume(&input).is_none());
    assert_eq!(
        clock::guard(receiver(), response(), 201, 200).err(),
        Some(Error::OutsideTimeBounds)
    );
}
#[test]
fn nested_setup_checks_resume_in_order() {
    let first = clock::guard(receiver(), pending(), 50, 250).unwrap();
    let second = clock::resume(&reply(first, 150)).unwrap().unwrap();
    let result = clock::resume(&reply(second, 150)).unwrap().unwrap();
    assert_eq!(
        borsh::to_vec(&result).unwrap(),
        borsh::to_vec(&response()).unwrap()
    );
}

#[test]
fn guard_requires_a_final_state_and_rejects_truncated_pending_bytes() {
    assert_eq!(
        clock::guard(receiver(), Response::keep_state(), 0, 10).err(),
        Some(Error::InvalidState)
    );
    let mut input = reply(pending(), 150);
    input.pre_state = b"LZCLK001".to_vec().into();
    assert_eq!(
        clock::resume(&input).unwrap().err(),
        Some(Error::InvalidState)
    );
}
