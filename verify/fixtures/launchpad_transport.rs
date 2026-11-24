use std::{
    collections::{BTreeMap, VecDeque},
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
    account::{Actor, Nonce},
    program::{PROGRAM_LOADER_ACCOUNT_ID, ProgramHeader},
};
use program_loader_core::LoaderActorState as _;
use wallet::{ExecutionFailureKind, FeeParams, FeeStateQuote, WalletCore, WalletSequencerClient};

#[derive(Clone, Default)]
pub struct FixtureTransport {
    accounts: BTreeMap<AccountId, Account>,
    writes: Arc<Mutex<Writes>>,
}

#[derive(Default)]
struct Writes {
    responses: VecDeque<Result<(HashType, Option<TransactionReceipt>), TransportError>>,
    attempts: usize,
    sent: Vec<(HashType, LeeTransaction, Option<TransactionReceipt>)>,
}

impl FixtureTransport {
    pub fn save(&mut self, actor: Actor, bytes: Vec<u8>) {
        self.accounts
            .entry(actor.account_id)
            .or_default()
            .data
            .set_actor_state(actor.program_account_id, bytes.into());
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

pub fn write_wallet(
    home: &Path,
    mut transport: FixtureTransport,
) -> (WalletCore<FixtureTransport>, AccountId) {
    let key = lee::PrivateKey::try_new([1; 32]).unwrap();
    let signer = AccountId::from(&lee::PublicKey::new_from_private_key(&key));
    transport
        .accounts
        .insert(signer, Account::funded(1_000_000_000));
    let config_path = home.join("wallet_config.json");
    let config = serde_json::to_vec(&wallet::config::WalletConfig::default()).unwrap();
    std::fs::write(&config_path, config).unwrap();
    let (mut wallet, _) = WalletCore::new_init_storage_with_client(
        config_path,
        home.join("wallet_storage"),
        None,
        "",
        transport,
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
        Ok(self.accounts.get(&actor.account_id).cloned())
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
        Ok(self
            .writes
            .lock()
            .unwrap()
            .sent
            .iter()
            .find_map(|(known, _, receipt)| (*known == hash).then_some(*receipt).flatten()))
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
        Ok(self.accounts.get(&id).cloned())
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
