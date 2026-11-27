use std::io::{self, Write};

use common::HashType;
use launchpad_client::{LifecycleReceipt, NamespaceReceipt};
use lee::AccountId;
use serde_json::json;

#[derive(Debug)]
pub(super) struct WriteSuccess {
    hashes: Vec<HashType>,
    launch_salt: Option<[u8; 32]>,
    authority_holders: Vec<AccountId>,
    creation_stage: Option<&'static str>,
}

impl WriteSuccess {
    pub(super) fn new(hashes: Vec<HashType>) -> Self {
        Self {
            hashes,
            launch_salt: None,
            authority_holders: Vec::new(),
            creation_stage: None,
        }
    }

    pub(super) fn single(hash: HashType, launch_salt: Option<[u8; 32]>) -> Self {
        Self {
            launch_salt,
            ..Self::new(vec![hash])
        }
    }

    pub(super) fn namespace(receipt: NamespaceReceipt) -> Self {
        Self {
            authority_holders: receipt.new_admin.into_iter().collect(),
            ..Self::single(receipt.transaction_hash, None)
        }
    }

    pub(super) fn lifecycle(receipt: LifecycleReceipt, launch_salt: [u8; 32]) -> Self {
        Self {
            launch_salt: Some(launch_salt),
            authority_holders: receipt.authority_holders,
            ..Self::new(receipt.transaction_hashes)
        }
    }

    pub(super) fn creation(
        receipt: LifecycleReceipt,
        launch_salt: [u8; 32],
        stage: &'static str,
    ) -> Self {
        Self {
            creation_stage: Some(stage),
            ..Self::lifecycle(receipt, launch_salt)
        }
    }

    pub(super) fn write(self, json_output: bool, output: &mut impl Write) -> io::Result<()> {
        let hashes: Vec<_> = self.hashes.into_iter().map(hex::encode).collect();
        let salt = self.launch_salt.map(hex::encode);
        let holders: Vec<_> = self
            .authority_holders
            .iter()
            .map(ToString::to_string)
            .collect();
        let (status, submission) = if hashes.is_empty() {
            ("no_change", "not_sent")
        } else {
            ("executed", "acknowledged")
        };
        if json_output {
            let mut view = json!({
                "status": status,
                "submission": submission,
                "transaction_hash": hashes.last(),
                "executed_transaction_hashes": hashes,
                "launch_salt": salt,
                "authority_holders": holders,
            });
            if let Some(stage) = self.creation_stage {
                view["setup_stage"] = json!(stage);
                view["funded"] = json!(true);
            }
            return writeln!(output, "{view}");
        }
        write!(output, "{status}: submission={submission}")?;
        if let Some(salt) = salt {
            write!(output, " launch_salt={salt}")?;
        }
        if let Some(stage) = self.creation_stage {
            write!(output, " setup_stage={stage} funded=true")?;
        }
        writeln!(output)?;
        for (index, hash) in hashes.iter().enumerate() {
            write!(output, "tx_hash={hash}")?;
            if let Some(holder) = holders.get(index) {
                write!(output, " authority_holder=Public/{holder}")?;
            }
            writeln!(output)?;
        }
        Ok(())
    }
}
