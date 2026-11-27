use lee::AccountId;

use crate::{
    ClientError, ProgramHandle, WriteTransport, build_initialize_namespace_invocation,
    submit_public_invocation,
};

use super::launchpad_transport;

#[tokio::test(start_paused = true)]
async fn public_write_stays_pending_without_a_receipt_even_when_included() {
    let program = ProgramHandle {
        account_id: AccountId::new([42; 32]),
        image: programs::clock(),
    };
    let hash = common::HashType([21; 32]);
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.deploy(program.account_id, program.image_id());
    transport.acknowledge(hash, None);
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let call = build_initialize_namespace_invocation(signer, program.account_id, 0, signer);

    let result = submit_public_invocation(&wallet, &program, call).await;
    assert!(
        matches!(result, Err(ClientError::PublicWritePending { hash: Some(known), .. }) if known == hash),
        "{result:?}"
    );
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn public_write_keeps_the_hash_when_receipt_lookup_fails() {
    let program = ProgramHandle {
        account_id: AccountId::new([42; 32]),
        image: programs::clock(),
    };
    let hash = common::HashType([24; 32]);
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.deploy(program.account_id, program.image_id());
    transport.acknowledge(hash, None);
    transport.fail_receipt_lookup();
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let call = build_initialize_namespace_invocation(signer, program.account_id, 0, signer);

    let result = submit_public_invocation(&wallet, &program, call).await;
    assert!(
        matches!(result, Err(ClientError::PublicWritePending { hash: Some(known), .. }) if known == hash),
        "{result:?}"
    );
    assert_eq!(transport.sent_count(), 1);
}

#[tokio::test]
async fn public_write_with_a_lost_send_response_is_pending_without_a_hash() {
    let program = ProgramHandle {
        account_id: AccountId::new([42; 32]),
        image: programs::clock(),
    };
    let mut transport = launchpad_transport::FixtureTransport::default();
    transport.deploy(program.account_id, program.image_id());
    transport.lose_send_response();
    let home = tempfile::tempdir().unwrap();
    let (wallet, signer) =
        launchpad_transport::write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let call = build_initialize_namespace_invocation(signer, program.account_id, 0, signer);

    let result = submit_public_invocation(&wallet, &program, call).await;
    assert!(
        matches!(
            result,
            Err(ClientError::PublicWritePending { hash: None, .. })
        ),
        "{result:?}"
    );
    assert_eq!(transport.sent_count(), 1);
}
