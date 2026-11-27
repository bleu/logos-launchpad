use common::HashType;
use curve_core::AdminCandidate;
use lee::AccountId;
use wallet::WalletSequencerClient;

use crate::{
    ClientError, ProgramHandle, PublicWriteWallet, Result, build_initialize_namespace_invocation,
    build_renounce_admin_invocation, build_transfer_admin_invocation, build_update_fee_invocation,
    build_update_treasury_invocation, submit_public_invocation,
};

#[derive(Debug, Clone, Copy)]
pub enum NamespaceAction {
    Initialize {
        protocol_fee_bps: u16,
        treasury: AccountId,
    },
    UpdateFee {
        admin: AccountId,
        protocol_fee_bps: u16,
    },
    UpdateTreasury {
        admin: AccountId,
        treasury: AccountId,
    },
    /// The current administrator and successor sign the same transaction.
    TransferAdmin {
        admin: AccountId,
        new_admin: AccountId,
    },
    /// Permanently remove the namespace administrator. The emergency role stays in place.
    RenounceAdmin { admin: AccountId },
}

#[derive(Debug, Clone, Copy)]
pub struct NamespaceReceipt {
    pub transaction_hash: HashType,
    /// The administrator installed by this transfer, if the action was a transfer.
    pub new_admin: Option<AccountId>,
}

/// Submits one public namespace action and waits for a successful execution receipt.
pub async fn submit_namespace_action(
    wallet: &PublicWriteWallet<impl WalletSequencerClient>,
    program: &ProgramHandle,
    namespace: AccountId,
    action: NamespaceAction,
) -> Result<NamespaceReceipt> {
    let call = match action {
        NamespaceAction::Initialize {
            protocol_fee_bps,
            treasury,
        } => build_initialize_namespace_invocation(
            namespace,
            program.account_id,
            protocol_fee_bps,
            treasury,
        ),
        NamespaceAction::UpdateFee {
            admin,
            protocol_fee_bps,
        } => build_update_fee_invocation(namespace, program.account_id, admin, protocol_fee_bps),
        NamespaceAction::UpdateTreasury { admin, treasury } => {
            build_update_treasury_invocation(namespace, program.account_id, admin, treasury)
        }
        NamespaceAction::TransferAdmin { admin, new_admin } => build_transfer_admin_invocation(
            namespace,
            program.account_id,
            admin,
            new_admin,
            AdminCandidate::Signer,
        ),
        NamespaceAction::RenounceAdmin { admin } => {
            build_renounce_admin_invocation(namespace, program.account_id, admin)
        }
    };
    for &signer in &call.signer_accounts {
        if wallet.get_account_public_signing_key(signer).is_none() {
            return Err(ClientError::PublicSigningKeyUnavailable(signer));
        }
    }
    let transaction_hash = submit_public_invocation(wallet, program, call).await?;
    let new_admin = match action {
        NamespaceAction::TransferAdmin { new_admin, .. } => Some(new_admin),
        _ => None,
    };
    Ok(NamespaceReceipt {
        transaction_hash,
        new_admin,
    })
}
