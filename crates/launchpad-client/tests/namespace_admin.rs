use common::{HashType, block::TransactionReceipt, transaction::LeeTransaction};
use curve_core::Message;
use launchpad_client::{
    ClientError, NamespaceAction, ProgramHandle, WriteTransport, load_curve_config,
    submit_namespace_action,
};
use lee::AccountId;
use lee_core::account::Actor;
use wallet::WalletSequencerClient;

#[allow(dead_code)]
#[path = "../../../verify/fixtures/launchpad_transport.rs"]
mod launchpad_transport;
use launchpad_transport::{FixtureTransport, write_wallet};

fn deployment(transport: &mut FixtureTransport) -> ProgramHandle {
    let curve = ProgramHandle {
        account_id: AccountId::new([7; 32]),
        image: programs::clock(),
    };
    transport.deploy(curve.account_id, curve.image_id());
    curve
}

async fn submitted(transport: &FixtureTransport, hash: HashType) -> LeeTransaction {
    transport.get_transaction(hash).await.unwrap().unwrap().0
}

#[tokio::test]
async fn initialization_sends_the_namespace_settings_with_its_signer() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let hash = HashType([11; 32]);
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (wallet, namespace) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let treasury = AccountId::new([4; 32]);

    let receipt = submit_namespace_action(
        &wallet,
        &curve,
        namespace,
        NamespaceAction::Initialize {
            protocol_fee_bps: 25,
            treasury,
        },
    )
    .await
    .unwrap();

    assert_eq!(receipt.transaction_hash, hash);
    let tx = submitted(&transport, hash).await;
    let (destination, bytes) = tx.public_call().unwrap();
    assert_eq!(destination, Actor::new(namespace, curve.account_id));
    assert!(matches!(
        borsh::from_slice::<Message>(bytes).unwrap(),
        Message::InitializeNamespace {
            namespace: saved_namespace,
            protocol_fee_bps: 25,
            treasury: saved_treasury,
        } if saved_namespace == namespace && saved_treasury == treasury
    ));
}

#[tokio::test]
async fn config_reads_use_saved_settings_and_reject_a_namespace_mismatch() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let namespace = AccountId::new([5; 32]);
    let other_namespace = AccountId::new([6; 32]);
    let treasury = AccountId::new([4; 32]);
    let state = borsh::to_vec(&(
        *b"LZNSCONF",
        3_u16,
        namespace,
        175_u16,
        treasury,
        AccountId::new([3; 32]),
        false,
    ))
    .unwrap();
    for account_namespace in [namespace, other_namespace] {
        transport.save(
            Actor::new(
                curve_core::compute_config_pda(account_namespace, curve.account_id),
                curve.account_id,
            ),
            state.clone(),
        );
    }
    let home = tempfile::tempdir().unwrap();
    let (wallet, _) = write_wallet(home.path(), transport, WriteTransport::new);

    let config = load_curve_config(namespace, &wallet, curve.account_id)
        .await
        .unwrap();
    assert_eq!(config.protocol_fee_bps, 175);
    assert_eq!(config.treasury, treasury);
    assert!(matches!(
        load_curve_config(other_namespace, &wallet, curve.account_id).await,
        Err(ClientError::PoolSelectionMismatch)
    ));
}

#[tokio::test]
async fn fee_updates_send_only_the_requested_rate_even_when_repeated() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let hashes = [HashType([12; 32]), HashType([13; 32])];
    for hash in hashes {
        transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    }
    let home = tempfile::tempdir().unwrap();
    let (wallet, admin) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let namespace = AccountId::new([5; 32]);

    for hash in hashes {
        let receipt = submit_namespace_action(
            &wallet,
            &curve,
            namespace,
            NamespaceAction::UpdateFee {
                admin,
                protocol_fee_bps: 175,
            },
        )
        .await
        .unwrap();

        assert_eq!(receipt.transaction_hash, hash);
        let tx = submitted(&transport, hash).await;
        let (destination, bytes) = tx.public_call().unwrap();
        assert_eq!(destination, Actor::new(admin, curve.account_id));
        assert!(matches!(
            borsh::from_slice::<Message>(bytes).unwrap(),
            Message::UpdateFee { namespace: selected, protocol_fee_bps: 175 } if selected == namespace
        ));
    }
    assert_eq!(transport.sent_count(), 2);
}

#[tokio::test]
async fn treasury_updates_send_only_the_requested_destination() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let hash = HashType([14; 32]);
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (wallet, admin) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let namespace = AccountId::new([5; 32]);
    let treasury = AccountId::new([6; 32]);

    let receipt = submit_namespace_action(
        &wallet,
        &curve,
        namespace,
        NamespaceAction::UpdateTreasury { admin, treasury },
    )
    .await
    .unwrap();

    assert_eq!(receipt.transaction_hash, hash);
    let tx = submitted(&transport, hash).await;
    let (destination, bytes) = tx.public_call().unwrap();
    assert_eq!(destination, Actor::new(admin, curve.account_id));
    assert!(matches!(
        borsh::from_slice::<Message>(bytes).unwrap(),
        Message::UpdateTreasury { namespace: selected, treasury: saved }
            if selected == namespace && saved == treasury
    ));
}

#[tokio::test]
async fn fee_and_treasury_updates_return_an_error_for_reverted_receipts() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let home = tempfile::tempdir().unwrap();
    let (wallet, admin) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let namespace = AccountId::new([5; 32]);
    let reverted = TransactionReceipt {
        success: false,
        ..TransactionReceipt::EXEMPT
    };
    for (hash, action) in [
        (
            HashType([17; 32]),
            NamespaceAction::UpdateFee {
                admin,
                protocol_fee_bps: 175,
            },
        ),
        (
            HashType([18; 32]),
            NamespaceAction::UpdateTreasury {
                admin,
                treasury: AccountId::new([6; 32]),
            },
        ),
    ] {
        transport.acknowledge(hash, Some(reverted));
        let result = submit_namespace_action(&wallet, &curve, namespace, action).await;
        assert!(
            matches!(result, Err(ClientError::PublicWriteReverted { hash: actual }) if actual == hash),
            "{action:?}: {result:?}"
        );
    }
    assert_eq!(transport.sent_count(), 2);
}

#[tokio::test]
async fn renunciation_sends_the_selected_namespace_with_the_admin_signature() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let hash = HashType([16; 32]);
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (wallet, admin) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let namespace = AccountId::new([5; 32]);

    let receipt = submit_namespace_action(
        &wallet,
        &curve,
        namespace,
        NamespaceAction::RenounceAdmin { admin },
    )
    .await
    .unwrap();

    assert_eq!(receipt.transaction_hash, hash);
    assert_eq!(receipt.new_admin, None);
    let LeeTransaction::Public(tx) = submitted(&transport, hash).await else {
        panic!("expected a public transaction");
    };
    assert_eq!(tx.check_stateless().unwrap(), [admin]);
    let root = &tx.message().execution.root;
    assert_eq!(root.to, Actor::new(admin, curve.account_id));
    assert!(matches!(
        borsh::from_slice::<Message>(&root.message).unwrap(),
        Message::RenounceAdmin { namespace: selected } if selected == namespace
    ));
}

#[tokio::test]
async fn admin_transfer_requires_both_signers_on_one_transaction() {
    let mut transport = FixtureTransport::default();
    let curve = deployment(&mut transport);
    let hash = HashType([15; 32]);
    transport.acknowledge(hash, Some(TransactionReceipt::EXEMPT));
    let home = tempfile::tempdir().unwrap();
    let (mut wallet, admin) = write_wallet(home.path(), transport.clone(), WriteTransport::new);
    let namespace = AccountId::new([5; 32]);
    let key = lee::PrivateKey::try_new([2; 32]).unwrap();
    let new_admin = AccountId::from(&lee::PublicKey::new_from_private_key(&key));
    let action = NamespaceAction::TransferAdmin { admin, new_admin };

    let error = submit_namespace_action(&wallet, &curve, namespace, action)
        .await
        .unwrap_err();
    assert!(matches!(error, ClientError::PublicSigningKeyUnavailable(id) if id == new_admin));
    assert_eq!(transport.sent_count(), 0);

    wallet
        .storage_mut()
        .key_chain_mut()
        .add_imported_public_account(key);
    let receipt = submit_namespace_action(&wallet, &curve, namespace, action)
        .await
        .unwrap();
    assert_eq!(receipt.transaction_hash, hash);
    assert_eq!(receipt.new_admin, Some(new_admin));
    assert_eq!(transport.sent_count(), 1);

    let LeeTransaction::Public(tx) = submitted(&transport, hash).await else {
        panic!("expected a public transaction");
    };
    let mut signers = tx.check_stateless().unwrap();
    signers.sort_unstable();
    let mut expected = [admin, new_admin];
    expected.sort_unstable();
    assert_eq!(signers, expected);
    assert!(matches!(
        borsh::from_slice::<Message>(&tx.message().execution.root.message).unwrap(),
        Message::TransferAdmin {
            namespace: selected,
            new_admin: successor,
            candidate: curve_core::AdminCandidate::Signer,
        } if selected == namespace && successor == new_admin
    ));
}
