use crate::{
    ArtifactEvidence,
    native_execution::{Trial, initial_pre_states, programs},
};
use eyre::Result;
use lee::{
    PRIVACY_PRESERVING_CIRCUIT_ID, PrivacyPreservingTransaction, V03State,
    privacy_preserving_transaction::{circuit::Proof, message::Message, witness_set::WitnessSet},
};
use lee_core::{
    PrivacyPreservingCircuitOutput,
    account::{Account, AccountId},
};
use risc0_zkvm::{InnerReceipt, Receipt, SuccinctReceiptVerifierParameters, VerifierContext};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const PROVER_SHA256: &str = "e57ef8a01d97d1b87ca7f1fb7ab55eb636d478d7e5a8e498bbe985120c3e81f9";
const WALL_LIMIT: Duration = Duration::from_secs(1_800);
const RSS_LIMIT: u64 = 12 * 1024 * 1024 * 1024;
const WIRE_LIMIT: u64 = 4 * 1024 * 1024;

fn local_server() -> Result<PathBuf> {
    use risc0_zkvm::sha::{Impl, Sha256};
    crate::check_local_configuration().map_err(|_| opaque_error())?;
    if std::env::var_os("RISC0_PROVER").is_some_and(|value| value != "ipc")
        || std::env::var_os("RISC0_DEV_MODE").is_some_and(|value| value != "0")
    {
        return Err(opaque_error());
    }
    let requested = std::env::var_os("RISC0_SERVER_PATH").unwrap_or_else(|| "r0vm".into());
    let requested = PathBuf::from(requested);
    let path = if requested.components().count() > 1 {
        requested
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(&requested))
            .find(|path| path.is_file())
            .ok_or_else(opaque_error)?
    };
    let path = path.canonicalize().map_err(|_| opaque_error())?;
    let bytes = std::fs::read(&path).map_err(|_| opaque_error())?;
    if hex::encode(Impl::hash_bytes(&bytes).as_bytes()) != PROVER_SHA256 {
        return Err(opaque_error());
    }
    Ok(path)
}

fn wire_encode(value: &impl Serialize) -> Result<Vec<u8>> {
    let bytes: Vec<_> = risc0_zkvm::serde::to_vec(value)
        .map_err(|_| opaque_error())?
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    if bytes.len() as u64 > WIRE_LIMIT {
        return Err(opaque_error());
    }
    Ok(bytes)
}

pub fn run_worker() -> Result<()> {
    local_server()?;
    std::panic::set_hook(Box::new(|_| {
        eprintln!("native worker failed; witness details are not logged")
    }));
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(WIRE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| opaque_error())?;
    if bytes.len() as u64 > WIRE_LIMIT || bytes.len() % 4 != 0 {
        return Err(opaque_error());
    }
    let trial: Trial = risc0_zkvm::serde::from_slice(&bytes).map_err(|_| opaque_error())?;
    if !matches!(trial.caller, crate::native_execution::Caller::Eligibility) {
        return Err(opaque_error());
    }
    let graph = programs()?;
    let pre_states = initial_pre_states(&trial)?;
    let instruction = crate::native_execution::root_instruction(&trial)?;
    let npk = lee_core::NullifierPublicKey::from(&crate::native_execution::SCRATCH_SECRET_KEY);
    let vpk = lee_core::encryption::ViewingPublicKey::from_seed(&[0x63; 32], &[0x64; 32]);
    let (ssk, epk) = lee_core::SharedSecretKey::encapsulate(&vpk);
    let identities = vec![
        lee_core::InputAccountIdentity::Public,
        lee_core::InputAccountIdentity::Public,
        lee_core::InputAccountIdentity::Public,
        lee_core::InputAccountIdentity::PrivateAuthorizedInit {
            epk,
            view_tag: lee_core::EncryptedAccountData::compute_view_tag(&npk, &vpk),
            ssk,
            nsk: crate::native_execution::SCRATCH_SECRET_KEY,
            identifier: crate::native_execution::SCRATCH_IDENTIFIER,
        },
    ];
    let started = Instant::now();
    let (output, proof) = lee::privacy_preserving_transaction::circuit::execute_and_prove(
        pre_states,
        instruction,
        identities,
        &graph,
    )
    .map_err(|_| opaque_error())?;
    let prove_seconds = started.elapsed().as_secs_f64();
    let inner = borsh::from_slice(&proof.into_inner()).map_err(|_| opaque_error())?;
    let mut artifact = NativeArtifact::from_outer_receipt(Receipt::new(inner, output.to_bytes()));
    artifact.prove_seconds = prove_seconds;
    verified_output(&artifact)?;
    std::io::stdout()
        .write_all(&wire_encode(&artifact)?)
        .map_err(|_| opaque_error())?;
    Ok(())
}

struct Worker(Child);

impl Drop for Worker {
    fn drop(&mut self) {
        let group = format!("-{}", self.0.id());
        let _ = Command::new("/bin/kill")
            .args(["-KILL", "--", &group])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.0.wait();
    }
}

fn group_rss(group: u32) -> Result<u64> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pgid=,rss="])
        .output()
        .map_err(|_| opaque_error())?;
    if !output.status.success() {
        return Err(opaque_error());
    }
    let output = std::str::from_utf8(&output.stdout).map_err(|_| opaque_error())?;
    let mut total = 0_u64;
    for line in output.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2 {
            return Err(opaque_error());
        }
        let id: u32 = fields[0].parse().map_err(|_| opaque_error())?;
        let rss: u64 = fields[1].parse().map_err(|_| opaque_error())?;
        if id == group {
            total = total
                .checked_add(rss.checked_mul(1024).ok_or_else(opaque_error)?)
                .ok_or_else(opaque_error)?;
        }
    }
    Ok(total)
}

fn inventory(path: &Path) -> Result<(usize, u64)> {
    let mut files = 0;
    let mut bytes = 0;
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok((0, 0)),
        Err(_) => return Err(opaque_error()),
    };
    for entry in entries {
        let entry = entry.map_err(|_| opaque_error())?;
        let metadata = match entry.path().symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(opaque_error()),
        };
        if metadata.is_symlink() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(eyre::eyre!("unsafe native artifact permissions or symlink"));
        }
        if metadata.is_dir() {
            let (nested_files, nested_bytes) = inventory(&entry.path())?;
            files += nested_files;
            bytes += nested_bytes;
        } else if metadata.is_file() {
            files += 1;
            bytes += metadata.len();
        }
    }
    Ok((files, bytes))
}

pub fn prove(trial: &Trial) -> Result<NativeArtifact> {
    let server = local_server()?;
    let bytes = wire_encode(trial)?;
    let executable = std::env::current_exe().map_err(|_| opaque_error())?;
    let executable = if executable
        .parent()
        .is_some_and(|path| path.ends_with("deps"))
    {
        executable
            .parent()
            .and_then(Path::parent)
            .ok_or_else(opaque_error)?
            .join("eligibility-host")
    } else {
        executable
    };
    let directory = crate::protected_segments().map_err(|_| opaque_error())?;
    let path = directory.path().to_owned();
    let started = Instant::now();
    let mut worker = Worker(
        Command::new("/bin/sh")
            .args([
                "-c",
                "ulimit -c 0 || exit 1; umask 077; exec \"$@\"",
                "native-worker",
            ])
            .arg(executable)
            .arg("native-child")
            .env("TMPDIR", &path)
            .env("RISC0_SERVER_PATH", server)
            .env("RISC0_PROVER", "ipc")
            .env("RISC0_DEV_MODE", "0")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()
            .map_err(|_| opaque_error())?,
    );
    let stdin = worker.0.stdin.take().ok_or_else(opaque_error)?;
    let stdout = worker.0.stdout.take().ok_or_else(opaque_error)?;
    let (sent, received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let result = (|| {
            let mut bytes = Vec::new();
            stdout
                .take(WIRE_LIMIT + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| opaque_error())?;
            if bytes.len() as u64 > WIRE_LIMIT {
                return Err(opaque_error());
            }
            Ok(bytes)
        })();
        let _ = sent.send(result);
    });
    let (input_sent, input_received) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut stdin = stdin;
        let _ = input_sent.send(stdin.write_all(&bytes));
    });
    let mut peak_rss = 0;
    let mut peak_files = 0;
    let mut peak_bytes = 0;
    let status = loop {
        if started.elapsed() >= WALL_LIMIT {
            return Err(eyre::eyre!("native proof wall bound reached"));
        }
        if input_received
            .try_recv()
            .is_ok_and(|result| result.is_err())
        {
            return Err(opaque_error());
        }
        peak_rss = peak_rss.max(group_rss(worker.0.id())?);
        if peak_rss > RSS_LIMIT {
            return Err(eyre::eyre!(
                "native proof sampled process-group RSS bound reached"
            ));
        }
        let (files, bytes) = inventory(&path)?;
        peak_files = peak_files.max(files);
        peak_bytes = peak_bytes.max(bytes);
        if let Some(status) = worker.0.try_wait().map_err(|_| opaque_error())? {
            break status;
        }
        std::thread::sleep(Duration::from_secs(2));
    };
    drop(worker);
    if !status.success() {
        return Err(opaque_error());
    }
    let bytes = received
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| opaque_error())??;
    let mut artifact: NativeArtifact =
        risc0_zkvm::serde::from_slice(&bytes).map_err(|_| opaque_error())?;
    verified_output(&artifact)?;
    directory.close().map_err(|_| opaque_error())?;
    if path.try_exists().map_err(|_| opaque_error())? {
        return Err(opaque_error());
    }
    artifact.wall_seconds = started.elapsed().as_secs_f64();
    artifact.sampled_peak_group_rss_bytes = peak_rss;
    artifact.artifacts = ArtifactEvidence {
        directory_mode: 0o700,
        files: peak_files,
        bytes: peak_bytes,
        removed: true,
    };
    Ok(artifact)
}

#[derive(Clone, Serialize, Deserialize)]
pub struct NativeArtifact {
    pub receipt: Receipt,
    pub prove_seconds: f64,
    pub wall_seconds: f64,
    pub sampled_peak_group_rss_bytes: u64,
    pub artifacts: ArtifactEvidence,
}

impl NativeArtifact {
    pub fn from_outer_receipt(receipt: Receipt) -> Self {
        Self {
            receipt,
            prove_seconds: 0.0,
            wall_seconds: 0.0,
            sampled_peak_group_rss_bytes: 0,
            artifacts: ArtifactEvidence {
                directory_mode: 0,
                files: 0,
                bytes: 0,
                removed: false,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateSnapshot {
    pub marker: Account,
    pub purchase: Account,
    pub commitment_root: [u8; 32],
}

pub struct TrialState {
    state: V03State,
    marker_id: AccountId,
    purchase_id: AccountId,
}

impl TrialState {
    pub fn new(trial: &Trial) -> Result<Self> {
        let graph = programs()?;
        if trial.config_account_id != eligibility_core::native::CONFIG_ACCOUNT_ID
            || trial.config_owner != eligibility_methods::NATIVE_PURCHASE_RECEIVER_ID
            || trial.trusted_eligibility_id != graph.program.id()
        {
            return Err(opaque_error());
        }
        let pre_states = initial_pre_states(trial)?;
        let accounts = pre_states[..3]
            .iter()
            .map(|pre| (pre.account_id, pre.account.clone()));
        let state = V03State::new()
            .with_public_accounts(accounts)
            .with_programs(std::iter::once(graph.program).chain(graph.dependencies.into_values()));
        Ok(Self {
            state,
            marker_id: pre_states[1].account_id,
            purchase_id: pre_states[2].account_id,
        })
    }

    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            marker: self.state.get_account_by_id(self.marker_id),
            purchase: self.state.get_account_by_id(self.purchase_id),
            commitment_root: self.state.commitment_set_digest(),
        }
    }
}

pub struct TransitionEvidence {
    pub state: StateSnapshot,
    pub proof_bytes: usize,
    pub transaction_bytes: usize,
    pub transaction: Vec<u8>,
    pub journal_bytes: usize,
    pub commitments: usize,
    pub nullifiers: usize,
}

fn opaque_error() -> eyre::Report {
    eyre::eyre!("native validation failed; witness details are not logged")
}

fn verified_output(artifact: &NativeArtifact) -> Result<PrivacyPreservingCircuitOutput> {
    if !matches!(&artifact.receipt.inner, InnerReceipt::Succinct(_)) {
        return Err(opaque_error());
    }
    let context = VerifierContext::empty()
        .with_dev_mode(false)
        .with_suites(VerifierContext::default_hash_suites())
        .with_succinct_verifier_parameters(SuccinctReceiptVerifierParameters::default());
    artifact
        .receipt
        .verify_with_context(&context, PRIVACY_PRESERVING_CIRCUIT_ID)
        .map_err(|_| opaque_error())?;
    let output: PrivacyPreservingCircuitOutput = artifact
        .receipt
        .journal
        .decode()
        .map_err(|_| opaque_error())?;
    if output.to_bytes() != artifact.receipt.journal.bytes {
        return Err(opaque_error());
    }
    Ok(output)
}

pub fn validate_and_apply(
    state: &mut TrialState,
    artifact: &NativeArtifact,
) -> Result<TransitionEvidence> {
    let output = verified_output(artifact)?;
    let ids = output
        .public_pre_states
        .iter()
        .map(|pre| pre.account_id)
        .collect();
    let proof_bytes = borsh::to_vec(&artifact.receipt.inner).map_err(|_| opaque_error())?;
    let proof_len = proof_bytes.len();
    let message =
        Message::try_from_circuit_output(ids, vec![], output).map_err(|_| opaque_error())?;
    let commitments = message.new_commitments.len();
    let nullifiers = message.new_nullifiers.len();
    let witness = WitnessSet::for_message(&message, Proof::from_inner(proof_bytes), &[]);
    let transaction = PrivacyPreservingTransaction::new(message, witness);
    let encoded_transaction = borsh::to_vec(&transaction).map_err(|_| opaque_error())?;
    let transaction_bytes = encoded_transaction.len();
    if transaction_bytes >= 1_048_576 {
        return Err(opaque_error());
    }
    state
        .state
        .transition_from_privacy_preserving_transaction(&transaction, 0, 0)
        .map_err(|_| opaque_error())?;
    Ok(TransitionEvidence {
        state: state.snapshot(),
        proof_bytes: proof_len,
        transaction_bytes,
        transaction: encoded_transaction,
        journal_bytes: artifact.receipt.journal.bytes.len(),
        commitments,
        nullifiers,
    })
}
