use std::{collections::BTreeMap, error::Error};

use common::{
    HashType, PageLimit,
    block::{Block, TransactionReceipt},
    transaction::LeeTransaction,
};
use lee::{Account, AccountId, EncryptedNote, ProgramId, PublicationsPage};
use lee_core::{
    BlockId, Commitment, CommitmentPosition, CommitmentSetDigest, MembershipProof,
    account::{Actor, Nonce},
};
use wallet::{ExecutionFailureKind, FeeStateQuote, WalletCore, WalletSequencerClient};

/// A wallet that identifies errors from the sequencer send call.
pub type PublicWriteWallet<C> = WalletCore<WriteTransport<C>>;

/// Wrap a sequencer client with this type before opening a wallet for Launchpad writes.
#[derive(Clone)]
pub struct WriteTransport<C>(C);

impl<C> WriteTransport<C> {
    pub fn new(client: C) -> Self {
        Self(client)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransportError<E> {
    #[error(transparent)]
    Read(E),
    #[error(transparent)]
    Send(E),
}

#[derive(Debug, thiserror::Error)]
#[error("sending transaction to sequencer")]
struct SendAttemptFailed(#[source] Box<dyn Error + Send + Sync>);

impl<E> From<TransportError<E>> for ExecutionFailureKind
where
    E: Error + Into<Self> + Send + Sync + 'static,
{
    fn from(error: TransportError<E>) -> Self {
        match error {
            TransportError::Read(source) => source.into(),
            TransportError::Send(source) => {
                Self::SequencerError(SendAttemptFailed(Box::new(source)).into())
            }
        }
    }
}

pub(super) fn is_send_failure(error: &ExecutionFailureKind) -> bool {
    matches!(error, ExecutionFailureKind::SequencerError(source) if source.is::<SendAttemptFailed>())
}

impl<C: WalletSequencerClient> WalletSequencerClient for WriteTransport<C> {
    type Error = TransportError<C::Error>;

    async fn send_transaction(&self, tx: LeeTransaction) -> Result<HashType, Self::Error> {
        self.0
            .send_transaction(tx)
            .await
            .map_err(TransportError::Send)
    }

    async fn get_block(&self, id: BlockId) -> Result<Option<Block>, Self::Error> {
        self.0.get_block(id).await.map_err(TransportError::Read)
    }

    async fn get_block_range(
        &self,
        start: BlockId,
        end: BlockId,
    ) -> Result<Vec<Block>, Self::Error> {
        self.0
            .get_block_range(start, end)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_last_block_id(&self) -> Result<BlockId, Self::Error> {
        self.0
            .get_last_block_id()
            .await
            .map_err(TransportError::Read)
    }

    async fn get_account_balance(&self, id: AccountId) -> Result<u128, Self::Error> {
        self.0
            .get_account_balance(id)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_transaction(
        &self,
        hash: HashType,
    ) -> Result<Option<(LeeTransaction, BlockId)>, Self::Error> {
        self.0
            .get_transaction(hash)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_transaction_receipt(
        &self,
        hash: HashType,
    ) -> Result<Option<TransactionReceipt>, Self::Error> {
        self.0
            .get_transaction_receipt(hash)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_accounts_nonces(&self, ids: Vec<AccountId>) -> Result<Vec<Nonce>, Self::Error> {
        self.0
            .get_accounts_nonces(ids)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_account_view(&self, actor: Actor) -> Result<Option<Account>, Self::Error> {
        self.0
            .get_account_view(actor)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_proofs_and_root(
        &self,
        commitments: Vec<Commitment>,
        positions: Vec<CommitmentPosition>,
    ) -> Result<
        (
            Vec<Option<MembershipProof>>,
            Vec<Option<MembershipProof>>,
            CommitmentSetDigest,
        ),
        Self::Error,
    > {
        self.0
            .get_proofs_and_root(commitments, positions)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_fee_state(&self) -> Result<FeeStateQuote, Self::Error> {
        self.0.get_fee_state().await.map_err(TransportError::Read)
    }

    async fn get_account(&self, id: AccountId) -> Result<Option<Account>, Self::Error> {
        self.0.get_account(id).await.map_err(TransportError::Read)
    }

    async fn get_program_ids(&self) -> Result<BTreeMap<String, ProgramId>, Self::Error> {
        self.0.get_program_ids().await.map_err(TransportError::Read)
    }

    async fn get_publications(
        &self,
        from: CommitmentPosition,
        limit: PageLimit,
    ) -> Result<PublicationsPage, Self::Error> {
        self.0
            .get_publications(from, limit)
            .await
            .map_err(TransportError::Read)
    }

    async fn get_recovery_binding(
        &self,
        id: AccountId,
    ) -> Result<Option<EncryptedNote>, Self::Error> {
        self.0
            .get_recovery_binding(id)
            .await
            .map_err(TransportError::Read)
    }
}
