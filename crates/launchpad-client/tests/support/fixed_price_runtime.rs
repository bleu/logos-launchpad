use std::collections::{HashMap, HashSet, VecDeque};

use lee::{PublicTransaction, program::Program};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata},
    program::{
        ChainedCall, Claim, DEFAULT_PROGRAM_ID, ProgramId, ProgramOutput,
        compute_public_authorized_pdas, validate_execution,
    },
};
use risc0_zkvm::{ExecutorEnv, default_executor};

#[derive(Debug)]
pub struct Invocation {
    pub program: ProgramId,
    pub cycles: u64,
    pub emitted: usize,
}

#[derive(Debug)]
pub struct Diagnostic {
    pub accounts: HashMap<AccountId, Account>,
    pub invocations: Vec<Invocation>,
    pub failure: Option<(ProgramId, String)>,
}

// This diagnostic checks signed guest execution before runtime nonce increments.
pub fn diagnostic(
    programs: &[Program],
    initial_accounts: &HashMap<AccountId, Account>,
    tx: PublicTransaction,
    timestamp: u64,
    max_executions: usize,
) -> Diagnostic {
    assert!(tx.witness_set().is_valid_for(tx.message()));
    assert!(!tx.message().account_ids.is_empty());
    assert_eq!(
        tx.message()
            .account_ids
            .iter()
            .collect::<HashSet<_>>()
            .len(),
        tx.message().account_ids.len()
    );
    let signer_ids: Vec<_> = tx
        .witness_set()
        .signatures_and_public_keys()
        .iter()
        .map(|(_, key)| AccountId::from(key))
        .collect();
    assert_eq!(signer_ids.len(), tx.message().nonces.len());
    for (id, nonce) in signer_ids.iter().zip(&tx.message().nonces) {
        assert_eq!(
            initial_accounts.get(id).cloned().unwrap_or_default().nonce,
            *nonce
        );
    }
    let signers: HashSet<_> = signer_ids.into_iter().collect();
    let initial = ChainedCall {
        program_id: tx.message().program_id,
        instruction_data: tx.message().instruction_data.clone(),
        pre_states: tx
            .message()
            .account_ids
            .iter()
            .map(|id| {
                AccountWithMetadata::new(
                    initial_accounts.get(id).cloned().unwrap_or_default(),
                    signers.contains(id),
                    *id,
                )
            })
            .collect(),
        pda_seeds: vec![],
    };
    let mut queue = VecDeque::from([(initial, None, signers)]);
    let mut diagnostic = Diagnostic {
        accounts: initial_accounts.clone(),
        invocations: vec![],
        failure: None,
    };
    while let Some((call, caller, inherited)) = queue.pop_front() {
        assert!(
            diagnostic.invocations.len() < max_executions,
            "unexpected diagnostic graph growth"
        );
        let program = programs
            .iter()
            .find(|program| program.id() == call.program_id)
            .unwrap();
        let mut builder = ExecutorEnv::builder();
        builder.session_limit(Some(32 * 1024 * 1024));
        builder
            .write(&program.id())
            .unwrap()
            .write(&caller)
            .unwrap()
            .write(&call.pre_states)
            .unwrap()
            .write(&call.instruction_data)
            .unwrap();
        let session = match default_executor().execute(builder.build().unwrap(), program.elf()) {
            Ok(session) => session,
            Err(error) => {
                diagnostic.failure = Some((program.id(), error.to_string()));
                return diagnostic;
            }
        };
        let mut output: ProgramOutput = session.journal.decode().unwrap();
        assert_eq!(output.self_program_id, program.id());
        assert_eq!(output.caller_program_id, caller);
        assert_eq!(output.instruction_data, call.instruction_data);
        assert_eq!(output.pre_states, call.pre_states);
        let authorized_pdas = compute_public_authorized_pdas(caller, &call.pda_seeds);
        for pre in &output.pre_states {
            assert_eq!(
                pre.account,
                diagnostic
                    .accounts
                    .get(&pre.account_id)
                    .cloned()
                    .unwrap_or_default(),
                "stale diagnostic snapshot {}",
                pre.account_id
            );
            assert_eq!(
                pre.is_authorized,
                inherited.contains(&pre.account_id) || authorized_pdas.contains(&pre.account_id),
                "wrong diagnostic authorization {}",
                pre.account_id
            );
        }
        validate_execution(&output.pre_states, &output.post_states, program.id()).unwrap();
        assert!(output.block_validity_window.is_valid_for(1));
        assert!(output.timestamp_validity_window.is_valid_for(timestamp));
        for (pre, post) in output.pre_states.iter().zip(&mut output.post_states) {
            if let Some(claim) = post.required_claim() {
                assert_eq!(post.account().program_owner, DEFAULT_PROGRAM_ID);
                match claim {
                    Claim::Authorized => assert!(pre.is_authorized),
                    Claim::Pda(seed) => assert_eq!(
                        pre.account_id,
                        AccountId::for_public_pda(&program.id(), &seed)
                    ),
                }
                post.account_mut().program_owner = program.id();
            }
            diagnostic
                .accounts
                .insert(pre.account_id, post.account().clone());
        }
        let authorized: HashSet<_> = inherited
            .into_iter()
            .chain(
                output
                    .pre_states
                    .iter()
                    .filter(|pre| pre.is_authorized)
                    .map(|pre| pre.account_id),
            )
            .collect();
        diagnostic.invocations.push(Invocation {
            program: program.id(),
            cycles: session.cycles(),
            emitted: output.chained_calls.len(),
        });
        for child in output.chained_calls.into_iter().rev() {
            queue.push_front((child, Some(program.id()), authorized.clone()));
        }
    }
    for (id, post) in &diagnostic.accounts {
        let pre = initial_accounts.get(id).cloned().unwrap_or_default();
        if pre.program_owner == DEFAULT_PROGRAM_ID && pre != *post {
            assert_ne!(
                post.program_owner, DEFAULT_PROGRAM_ID,
                "unclaimed diagnostic account {id}"
            );
        }
    }
    assert_eq!(
        diagnostic
            .invocations
            .iter()
            .map(|call| call.emitted)
            .sum::<usize>()
            + 1,
        diagnostic.invocations.len()
    );
    diagnostic
}
