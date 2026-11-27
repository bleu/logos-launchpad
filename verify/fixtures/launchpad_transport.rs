use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    io,
    path::Path,
    sync::{Arc, Mutex},
};

use common::{
    HashType, PageLimit,
    block::{Block, TransactionReceipt},
    transaction::LeeTransaction,
};
use lee::{Account, AccountId, EncryptedNote, ProgramId, PublicationsPage};
use lee_core::{
    BlockId, Commitment, CommitmentPosition, CommitmentSetDigest, MembershipProof,
    account::{Actor, ActorState, Nonce},
    program::{PROGRAM_LOADER_ACCOUNT_ID, ProgramHeader, ReceiveInput},
};
use lez_compat_core::authority;
use program_loader_core::LoaderActorState as _;
use wallet::{ExecutionFailureKind, FeeParams, FeeStateQuote, WalletCore, WalletSequencerClient};

const DEFAULT_SIGNER_BALANCE: u128 = 1_000_000_000;

#[derive(Clone, Default)]
pub struct FixtureTransport {
    accounts: BTreeMap<AccountId, Account>,
    writes: Arc<Mutex<Writes>>,
    receipt_lookup_fails: bool,
    signer_balance: Option<u128>,
}

#[derive(Default)]
struct Writes {
    responses: VecDeque<Result<(HashType, Option<TransactionReceipt>), TransportError>>,
    attempts: usize,
    sent: Vec<(HashType, LeeTransaction, Option<TransactionReceipt>)>,
    pending_actor_states: HashMap<HashType, Vec<(Actor, ActorState)>>,
    committed_actor_states: BTreeMap<AccountId, BTreeMap<AccountId, ActorState>>,
}

impl FixtureTransport {
    pub fn save(&mut self, actor: Actor, bytes: Vec<u8>) {
        self.accounts
            .entry(actor.account_id)
            .or_default()
            .data
            .set_actor_state(actor.program_account_id, bytes.into());
    }

    /// Applies this actor state when a clone reads a successful receipt for `hash`.
    pub fn save_after_receipt(&mut self, hash: HashType, actor: Actor, bytes: Vec<u8>) {
        self.writes
            .lock()
            .unwrap()
            .pending_actor_states
            .entry(hash)
            .or_default()
            .push((actor, bytes.into()));
    }

    pub fn set_signer_balance(&mut self, balance: u128) {
        self.signer_balance = Some(balance);
    }

    pub fn acknowledge(&mut self, hash: HashType, receipt: Option<TransactionReceipt>) {
        self.writes
            .lock()
            .unwrap()
            .responses
            .push_back(Ok((hash, receipt)));
    }

    pub fn lose_send_response(&mut self) {
        self.writes
            .lock()
            .unwrap()
            .responses
            .push_back(Err(TransportError));
    }

    pub fn fail_receipt_lookup(&mut self) {
        self.receipt_lookup_fails = true;
    }

    pub fn sent_count(&self) -> usize {
        self.writes.lock().unwrap().attempts
    }

    pub fn deploy(&mut self, account_id: AccountId, image_id: ProgramId) {
        let header = ProgramHeader {
            image_id,
            program_first_segment: AccountId::new([77; 32]),
            immutable: true,
        };
        self.save(
            Actor::new(account_id, PROGRAM_LOADER_ACCOUNT_ID),
            header.to_loader_actor_state(),
        );
    }
}

#[allow(dead_code)]
pub fn creator_role_state(role: Actor, factory: Actor, holder: AccountId) -> Vec<u8> {
    let bootstrap = authority::handle(&ReceiveInput {
        receiver: role,
        from: Some(factory),
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: borsh::to_vec(&authority::Message::BootstrapTransferOnly { authority: holder })
            .unwrap(),
    })
    .unwrap();
    let consent = &bootstrap.sync_messages[0];
    let accepted = authority::handle(&ReceiveInput {
        receiver: consent.to,
        from: Some(role),
        is_authorized: true,
        pre_state: ActorState::empty(),
        message: consent.message.clone(),
    })
    .unwrap();
    let install = &accepted.sync_messages[0];
    authority::handle(&ReceiveInput {
        receiver: install.to,
        from: Some(consent.to),
        is_authorized: false,
        pre_state: bootstrap.post_state.unwrap(),
        message: install.message.clone(),
    })
    .unwrap()
    .post_state
    .unwrap()
    .into_inner()
}

pub fn write_wallet<C: WalletSequencerClient>(
    home: &Path,
    mut transport: FixtureTransport,
    wrap: impl FnOnce(FixtureTransport) -> C,
) -> (WalletCore<C>, AccountId) {
    let key = lee::PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&lee::PublicKey::new_from_private_key(&key));
    let balance = transport.signer_balance.unwrap_or(DEFAULT_SIGNER_BALANCE);
    transport.accounts.insert(signer, Account::funded(balance));
    let config_path = home.join("wallet_config.json");
    let config = serde_json::to_vec(&wallet::config::WalletConfig::default()).unwrap();
    std::fs::write(&config_path, config).unwrap();
    let (mut wallet, _) = WalletCore::new_init_storage_with_client(
        config_path,
        home.join("wallet_storage"),
        None,
        "",
        wrap(transport),
    )
    .unwrap();
    wallet
        .storage_mut()
        .key_chain_mut()
        .add_imported_public_account(key);
    // Pin test fees so the wallet can send without a guest simulation.
    wallet.set_fee_params(FeeParams {
        gas_limit: Some(wallet::DEFAULT_GAS_LIMIT),
        max_fee: Some(wallet::DEFAULT_MAX_FEE),
        ..FeeParams::default()
    });
    (wallet, signer)
}

#[derive(Debug, thiserror::Error)]
#[error("fixture transport lost the response")]
pub struct TransportError;

impl From<TransportError> for ExecutionFailureKind {
    fn from(error: TransportError) -> Self {
        Self::SequencerError(io::Error::other(error).into())
    }
}

impl WalletSequencerClient for FixtureTransport {
    type Error = TransportError;

    async fn get_account_view(&self, actor: Actor) -> Result<Option<Account>, Self::Error> {
        self.get_account(actor.account_id).await
    }

    async fn send_transaction(&self, tx: LeeTransaction) -> Result<HashType, Self::Error> {
        let mut writes = self.writes.lock().unwrap();
        writes.attempts += 1;
        let (hash, receipt) = writes
            .responses
            .pop_front()
            .expect("unexpected submission")?;
        writes.sent.push((hash, tx, receipt));
        Ok(hash)
    }

    async fn get_block(&self, _: BlockId) -> Result<Option<Block>, Self::Error> {
        unreachable!()
    }

    async fn get_block_range(&self, _: BlockId, _: BlockId) -> Result<Vec<Block>, Self::Error> {
        unreachable!()
    }

    async fn get_last_block_id(&self) -> Result<BlockId, Self::Error> {
        unreachable!()
    }

    async fn get_account_balance(&self, _: AccountId) -> Result<u128, Self::Error> {
        unreachable!()
    }

    async fn get_transaction(
        &self,
        hash: HashType,
    ) -> Result<Option<(LeeTransaction, BlockId)>, Self::Error> {
        Ok(self
            .writes
            .lock()
            .unwrap()
            .sent
            .iter()
            .find_map(|(known, tx, _)| (*known == hash).then(|| (tx.clone(), 1))))
    }

    async fn get_transaction_receipt(
        &self,
        hash: HashType,
    ) -> Result<Option<TransactionReceipt>, Self::Error> {
        if self.receipt_lookup_fails {
            return Err(TransportError);
        }
        let mut writes = self.writes.lock().unwrap();
        let receipt = writes
            .sent
            .iter()
            .find_map(|(known, _, receipt)| (*known == hash).then_some(*receipt).flatten());
        // A reverted receipt discards its queued state.
        if let Some(receipt) = receipt
            && let Some(states) = writes.pending_actor_states.remove(&hash)
            && receipt.success
        {
            for (actor, state) in states {
                writes
                    .committed_actor_states
                    .entry(actor.account_id)
                    .or_default()
                    .insert(actor.program_account_id, state);
            }
        }
        Ok(receipt)
    }

    async fn get_accounts_nonces(&self, _: Vec<AccountId>) -> Result<Vec<Nonce>, Self::Error> {
        unreachable!()
    }

    async fn get_proofs_and_root(
        &self,
        _: Vec<Commitment>,
        _: Vec<CommitmentPosition>,
    ) -> Result<
        (
            Vec<Option<MembershipProof>>,
            Vec<Option<MembershipProof>>,
            CommitmentSetDigest,
        ),
        Self::Error,
    > {
        Ok((Vec::new(), Vec::new(), [0; 32]))
    }

    async fn get_fee_state(&self) -> Result<FeeStateQuote, Self::Error> {
        Ok(FeeStateQuote {
            height: 1,
            base_fee_exec: 0,
            base_fee_stor: 0,
            next_base_fee_exec_floor: 0,
            next_base_fee_exec_ceiling: 0,
            next_base_fee_stor_floor: 0,
            next_base_fee_stor_ceiling: 0,
            max_gas_exec: u64::MAX,
            max_gas_stor: u64::MAX,
        })
    }

    async fn get_account(&self, id: AccountId) -> Result<Option<Account>, Self::Error> {
        let mut account = self.accounts.get(&id).cloned();
        if let Some(states) = self.writes.lock().unwrap().committed_actor_states.get(&id) {
            let account = account.get_or_insert_with(Account::default);
            for (program, state) in states {
                account.data.set_actor_state(*program, state.clone());
            }
        }
        Ok(account)
    }

    async fn get_program_ids(&self) -> Result<BTreeMap<String, ProgramId>, Self::Error> {
        unreachable!()
    }

    async fn get_publications(
        &self,
        _: CommitmentPosition,
        _: PageLimit,
    ) -> Result<PublicationsPage, Self::Error> {
        unreachable!()
    }

    async fn get_recovery_binding(
        &self,
        _: AccountId,
    ) -> Result<Option<EncryptedNote>, Self::Error> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lee::public_transaction::{Message, WitnessSet};

    async fn submit(
        transport: &mut FixtureTransport,
        actor: Actor,
        hash: HashType,
        receipt: Option<TransactionReceipt>,
    ) {
        let tx = LeeTransaction::Public(lee::PublicTransaction::new(
            Message::try_new(actor, [], BTreeMap::new(), ()).unwrap(),
            WitnessSet::from_raw_parts(Vec::new()),
        ));
        transport.acknowledge(hash, receipt);
        assert_eq!(transport.send_transaction(tx).await.unwrap(), hash);
    }

    #[tokio::test]
    async fn receipt_updates_only_its_states_for_all_clones_once() {
        let mut transport = FixtureTransport::default();
        let id = AccountId::new([10; 32]);
        let first = Actor::new(id, AccountId::new([20; 32]));
        let second = Actor::new(id, AccountId::new([30; 32]));
        transport.accounts.insert(id, Account::funded(10));
        transport.save(first, vec![1]);
        let reader = transport.clone();
        let first_hash = HashType([1; 32]);
        let second_hash = HashType([2; 32]);
        transport.save_after_receipt(first_hash, first, vec![2]);
        transport.save_after_receipt(first_hash, second, vec![3]);
        transport.save_after_receipt(second_hash, first, vec![4]);
        submit(
            &mut transport,
            first,
            first_hash,
            Some(TransactionReceipt::EXEMPT),
        )
        .await;
        submit(
            &mut transport,
            first,
            second_hash,
            Some(TransactionReceipt::EXEMPT),
        )
        .await;

        let account = reader.get_account_view(first).await.unwrap().unwrap();
        assert_eq!(
            account.data.actor_state(first.program_account_id).as_ref(),
            &[1]
        );
        assert!(
            account
                .data
                .actor_state(second.program_account_id)
                .is_empty()
        );

        reader.get_transaction_receipt(first_hash).await.unwrap();
        let account = transport.get_account(id).await.unwrap().unwrap();
        assert_eq!(
            account.data.actor_state(first.program_account_id).as_ref(),
            &[2]
        );
        assert_eq!(
            account.data.actor_state(second.program_account_id).as_ref(),
            &[3]
        );
        assert_eq!(account.data.native_balance().unwrap(), 10);

        transport
            .get_transaction_receipt(second_hash)
            .await
            .unwrap();
        reader.get_transaction_receipt(first_hash).await.unwrap();
        let account = reader.get_account_view(first).await.unwrap().unwrap();
        assert_eq!(
            account.data.actor_state(first.program_account_id).as_ref(),
            &[4]
        );
        assert_eq!(
            account.data.actor_state(second.program_account_id).as_ref(),
            &[3]
        );
    }

    #[tokio::test]
    async fn missing_and_reverted_receipts_do_not_publish_state() {
        let mut transport = FixtureTransport::default();
        let actor = Actor::new(AccountId::new([10; 32]), AccountId::new([20; 32]));
        let reverted = TransactionReceipt {
            success: false,
            ..TransactionReceipt::EXEMPT
        };
        for (hash, receipt) in [
            (HashType([1; 32]), None),
            (HashType([2; 32]), Some(reverted)),
        ] {
            transport.save_after_receipt(hash, actor, vec![1]);
            submit(&mut transport, actor, hash, receipt).await;
            assert_eq!(
                transport.get_transaction_receipt(hash).await.unwrap(),
                receipt
            );
            assert!(transport.get_account_view(actor).await.unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn write_wallet_can_pin_the_signer_balance_to_zero() {
        let mut empty = FixtureTransport::default();
        empty.set_signer_balance(0);
        for (transport, expected) in [
            (FixtureTransport::default(), DEFAULT_SIGNER_BALANCE),
            (empty, 0),
        ] {
            let home = tempfile::tempdir().unwrap();
            let (wallet, signer) = write_wallet(home.path(), transport, |transport| transport);
            let account = wallet
                .get_account_view(Actor::native_balance(signer))
                .await
                .unwrap()
                .unwrap();
            assert_eq!(account.data.native_balance().unwrap(), expected);
        }
    }
}
