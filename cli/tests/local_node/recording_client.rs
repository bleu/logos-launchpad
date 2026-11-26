use std::{
    collections::BTreeMap,
    fs::File,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use common::{
    HashType, PageLimit,
    block::{Block, TransactionReceipt},
    transaction::LeeTransaction,
};
use lee::{Account, AccountId, Actor, EncryptedNote, ProgramId, PublicationsPage};
use lee_core::{
    BlockId, Commitment, CommitmentPosition, CommitmentSetDigest, MembershipProof, account::Nonce,
};
use serde::{Deserialize, Serialize};
use wallet::{ExecutionFailureKind, FeeStateQuote, WalletSequencerClient};

#[derive(Deserialize, Serialize)]
pub(super) struct SavedTransaction {
    pub(super) hash: HashType,
    pub(super) transaction: LeeTransaction,
}

impl SavedTransaction {
    pub(super) fn read(path: &Path) -> eyre::Result<Self> {
        Ok(serde_json::from_reader(File::open(path)?)?)
    }
}

#[derive(Clone)]
pub(super) struct RecordingClient<C> {
    inner: C,
    record_path: PathBuf,
    pub(super) forwarded: Arc<AtomicUsize>,
}

impl<C> RecordingClient<C> {
    pub(super) fn new(inner: C, record_path: PathBuf) -> Self {
        Self {
            inner,
            record_path,
            forwarded: Arc::default(),
        }
    }

    fn save(&self, record: &SavedTransaction) -> eyre::Result<()> {
        let directory = self
            .record_path
            .parent()
            .ok_or_else(|| eyre::eyre!("record path has no parent"))?;
        let mut file = tempfile::NamedTempFile::new_in(directory)?;
        serde_json::to_writer(&mut file, record)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(&self.record_path)?;
        Ok(())
    }
}

impl<C: WalletSequencerClient> WalletSequencerClient for RecordingClient<C> {
    type Error = ExecutionFailureKind;

    async fn send_transaction(&self, tx: LeeTransaction) -> Result<HashType, Self::Error> {
        let record = SavedTransaction {
            hash: tx.hash(),
            transaction: tx.clone(),
        };
        self.save(&record).map_err(|err| {
            ExecutionFailureKind::SequencerClientError(format!("save signed transaction: {err}"))
        })?;
        let saved = SavedTransaction::read(&self.record_path).map_err(|err| {
            ExecutionFailureKind::SequencerClientError(format!("reopen signed transaction: {err}"))
        })?;
        if saved.hash != record.hash || saved.transaction != tx {
            return Err(ExecutionFailureKind::SequencerClientError(
                "saved transaction differs from the signed transaction".to_owned(),
            ));
        }
        self.forwarded.fetch_add(1, Ordering::Relaxed);
        self.inner.send_transaction(tx).await.map_err(Into::into)
    }

    async fn get_block(&self, block_id: BlockId) -> Result<Option<Block>, Self::Error> {
        self.inner.get_block(block_id).await.map_err(Into::into)
    }

    async fn get_block_range(
        &self,
        start: BlockId,
        end: BlockId,
    ) -> Result<Vec<Block>, Self::Error> {
        self.inner
            .get_block_range(start, end)
            .await
            .map_err(Into::into)
    }

    async fn get_last_block_id(&self) -> Result<BlockId, Self::Error> {
        self.inner.get_last_block_id().await.map_err(Into::into)
    }

    async fn get_account_balance(&self, account_id: AccountId) -> Result<u128, Self::Error> {
        self.inner
            .get_account_balance(account_id)
            .await
            .map_err(Into::into)
    }

    async fn get_transaction(
        &self,
        hash: HashType,
    ) -> Result<Option<(LeeTransaction, BlockId)>, Self::Error> {
        self.inner.get_transaction(hash).await.map_err(Into::into)
    }

    async fn get_transaction_receipt(
        &self,
        hash: HashType,
    ) -> Result<Option<TransactionReceipt>, Self::Error> {
        self.inner
            .get_transaction_receipt(hash)
            .await
            .map_err(Into::into)
    }

    async fn get_accounts_nonces(
        &self,
        accounts: Vec<AccountId>,
    ) -> Result<Vec<Nonce>, Self::Error> {
        self.inner
            .get_accounts_nonces(accounts)
            .await
            .map_err(Into::into)
    }

    async fn get_account_view(&self, actor: Actor) -> Result<Option<Account>, Self::Error> {
        self.inner.get_account_view(actor).await.map_err(Into::into)
    }

    async fn get_proofs_and_root(
        &self,
        commitments: Vec<Commitment>,
        message_positions: Vec<CommitmentPosition>,
    ) -> Result<
        (
            Vec<Option<MembershipProof>>,
            Vec<Option<MembershipProof>>,
            CommitmentSetDigest,
        ),
        Self::Error,
    > {
        self.inner
            .get_proofs_and_root(commitments, message_positions)
            .await
            .map_err(Into::into)
    }

    async fn get_fee_state(&self) -> Result<FeeStateQuote, Self::Error> {
        self.inner.get_fee_state().await.map_err(Into::into)
    }

    async fn get_account(&self, account: AccountId) -> Result<Option<Account>, Self::Error> {
        self.inner.get_account(account).await.map_err(Into::into)
    }

    async fn get_program_ids(&self) -> Result<BTreeMap<String, ProgramId>, Self::Error> {
        self.inner.get_program_ids().await.map_err(Into::into)
    }

    async fn get_publications(
        &self,
        from_position: CommitmentPosition,
        limit: PageLimit,
    ) -> Result<PublicationsPage, Self::Error> {
        self.inner
            .get_publications(from_position, limit)
            .await
            .map_err(Into::into)
    }

    async fn get_recovery_binding(
        &self,
        address: AccountId,
    ) -> Result<Option<EncryptedNote>, Self::Error> {
        self.inner
            .get_recovery_binding(address)
            .await
            .map_err(Into::into)
    }
}
