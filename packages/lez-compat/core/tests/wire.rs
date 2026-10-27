use lee_core::account::AccountId;
use lez_compat_core::{authority, token};

#[test]
fn token_transfer_matches_the_documented_borsh_layout() {
    let expected = [
        vec![2],
        vec![1; 32],
        vec![2; 32],
        vec![30, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    ]
    .concat();
    let message = token::Message::Transfer {
        definition: AccountId::new([1; 32]),
        recipient: AccountId::new([2; 32]),
        amount: 30,
    };
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<token::Message>(&expected).unwrap(),
        message
    );
}

#[test]
fn transfer_only_bootstrap_has_a_separate_wire_tag() {
    let expected = [vec![12], vec![2; 32]].concat();
    let message = authority::Message::BootstrapTransferOnly {
        authority: AccountId::new([2; 32]),
    };
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<authority::Message>(&expected).unwrap(),
        message
    );
}

#[test]
fn authority_transfer_and_renounce_match_the_documented_borsh_layout() {
    let expected = [vec![5], vec![1; 32], vec![2; 32]].concat();
    let message = authority::Message::Transfer {
        role: AccountId::new([1; 32]),
        successor: AccountId::new([2; 32]),
    };
    assert_eq!(borsh::to_vec(&message).unwrap(), expected);
    assert_eq!(
        borsh::from_slice::<authority::Message>(&expected).unwrap(),
        message
    );
    assert_eq!(
        borsh::to_vec(&authority::Message::Renounce {
            role: AccountId::new([1; 32])
        })
        .unwrap(),
        [vec![7], vec![1; 32]].concat()
    );
}
