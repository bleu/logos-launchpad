mod benchmark;

use eligibility_core::{TreeHash, check_eligibility};
use eyre::Result;
use std::time::Instant;

fn main() {
    if eligibility_host::check_local_configuration().is_err() {
        eprintln!("unsafe local proving configuration");
        std::process::exit(1);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [action, hash, account, mode] = args.as_slice()
        && action == "prove"
        && ["sha256", "poseidon"].contains(&hash.as_str())
        && ["public", "private"].contains(&account.as_str())
        && ["composite", "succinct"].contains(&mode.as_str())
    {
        if std::env::var_os("RISC0_DEV_MODE").is_some_and(|value| value != "0") {
            eprintln!("development proving is disabled");
            std::process::exit(1);
        }
        let hash = if hash == "sha256" {
            TreeHash::Sha256
        } else {
            TreeHash::Poseidon
        };
        let mode = if mode == "composite" {
            eligibility_host::ReceiptMode::Composite
        } else {
            eligibility_host::ReceiptMode::Succinct
        };
        if run_proof(hash, account, mode).is_err() {
            eprintln!("local proof failed; witness details are not logged");
            std::process::exit(1);
        }
        return;
    }
    let hash = match args.as_slice() {
        [action, hash] if action == "execute" && hash == "sha256" => TreeHash::Sha256,
        [action, hash] if action == "execute" && hash == "poseidon" => TreeHash::Poseidon,
        _ => {
            eprintln!(
                "usage: eligibility-host execute <sha256|poseidon> | prove <sha256|poseidon> <public|private> <composite|succinct>"
            );
            std::process::exit(1);
        }
    };
    if run(hash).is_err() {
        eprintln!("measurement failed; witness details are not logged");
        std::process::exit(1);
    }
}

fn run_proof(hash: TreeHash, account: &str, mode: eligibility_host::ReceiptMode) -> Result<()> {
    let prepared = benchmark::prepare(hash)?;
    let input = if account == "public" {
        &prepared.public
    } else {
        &prepared.private
    };
    let evidence = eligibility_host::prove(input, mode)?;
    let kind = match &evidence.receipt.inner {
        risc0_zkvm::InnerReceipt::Composite(_) => "composite",
        risc0_zkvm::InnerReceipt::Succinct(_) => "succinct",
        _ => eyre::bail!("unexpected receipt kind"),
    };
    println!(
        "hash={hash:?} account={account} accounts=100000 depth=17 receipt={kind} verified=true prove_seconds={:.6} verify_seconds={:.6} user_cycles={} total_cycles={} segments={} journal_bytes={}",
        evidence.prove_seconds,
        evidence.verify_seconds,
        evidence.user_cycles,
        evidence.total_cycles,
        evidence.segments,
        evidence.receipt.journal.bytes.len()
    );
    let image: Vec<_> = eligibility_methods::ELIGIBILITY_GUEST_ID
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    println!("image_id={}", hex::encode(image));
    println!(
        "artifact_directory_mode={:o} artifact_files={} artifact_bytes={} artifacts_removed={}",
        evidence.artifacts.directory_mode,
        evidence.artifacts.files,
        evidence.artifacts.bytes,
        evidence.artifacts.removed
    );
    Ok(())
}

fn run(hash: TreeHash) -> Result<()> {
    let prepared = benchmark::prepare(hash)?;
    let image: Vec<_> = eligibility_methods::ELIGIBILITY_GUEST_ID
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    println!(
        "hash={hash:?} accounts=100000 depth=17 capacity=131072 image_id={}",
        hex::encode(image)
    );
    println!(
        "account_seconds={:.6} tree_seconds={:.6} membership_seconds={:.6}",
        prepared.account_seconds, prepared.tree_seconds, prepared.membership_seconds
    );
    for (account, input) in [("public", &prepared.public), ("private", &prepared.private)] {
        let started = Instant::now();
        let expected = check_eligibility(&input.policy, &input.purchase, &input.witness)
            .map_err(|_| eyre::eyre!("synthetic eligibility failed"))?;
        let checker_seconds = started.elapsed().as_secs_f64();
        let started = Instant::now();
        let execution = eligibility_host::execute(input)?;
        if execution.permit.as_ref() != Some(&expected) {
            eyre::bail!("guest permit mismatch");
        }
        println!(
            "account={account} checker_seconds={checker_seconds:.6} execute_seconds={:.6} user_cycles={} padded_cycles={} segments={} permit=true",
            started.elapsed().as_secs_f64(),
            execution.user_cycles,
            execution.padded_cycles,
            execution.segments
        );
        println!(
            "artifact_directory_mode={:o} artifact_files={} artifact_bytes={} artifacts_removed={}",
            execution.artifacts.directory_mode,
            execution.artifacts.files,
            execution.artifacts.bytes,
            execution.artifacts.removed
        );
    }
    Ok(())
}
