use eligibility_core::{
    EligibilityInput, Purchase, SalePolicy, check_eligibility,
    native::{
        CONFIG_ACCOUNT_ID, EligibilityInstruction, MockPurchaseState, PurchaseInstruction,
        TrustedConfig, decode, encode,
    },
};
use eyre::Result;
use lee::{privacy_preserving_transaction::circuit::ProgramWithDependencies, program::Program};
use lee_core::{
    NullifierPublicKey,
    account::{Account, AccountId, AccountWithMetadata},
    program::{ChainedCall, MAX_NUMBER_CHAINED_CALLS, PdaSeed, ProgramId, ProgramOutput},
};
use risc0_zkvm::{Executor, ExecutorEnv, ExternalProver};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, collections::VecDeque, time::Instant};

pub const PUBLIC_EXECUTION_CAP: u64 = 33_554_432;
pub const SCRATCH_IDENTIFIER: u128 = 999;
pub const SCRATCH_SECRET_KEY: [u8; 32] = [0x62; 32];

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum Caller {
    Eligibility,
    Direct,
    Foreign,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Trial {
    pub input: EligibilityInput,
    pub trusted_policy: SalePolicy,
    pub receiver_purchase: Purchase,
    pub marker_account: Account,
    pub marker_account_id: AccountId,
    pub purchase_account: Account,
    pub config_account_id: AccountId,
    pub config_owner: ProgramId,
    pub trusted_eligibility_id: ProgramId,
    pub caller: Caller,
    pub fail_purchase: bool,
    pub scratch_account_id: AccountId,
}

impl Trial {
    pub fn new(input: EligibilityInput) -> Self {
        let permit = check_eligibility(&input.policy, &input.purchase, &input.witness).ok();
        let marker_account_id = AccountId::for_public_pda(
            &eligibility_methods::NATIVE_ELIGIBILITY_ID,
            &PdaSeed::new(permit.map_or([0; 32], |permit| permit.nullifier)),
        );
        Self {
            trusted_policy: input.policy.clone(),
            receiver_purchase: input.purchase,
            input,
            marker_account: Account::default(),
            marker_account_id,
            purchase_account: Account {
                program_owner: eligibility_methods::NATIVE_PURCHASE_RECEIVER_ID,
                ..Account::default()
            },
            config_account_id: CONFIG_ACCOUNT_ID,
            config_owner: eligibility_methods::NATIVE_PURCHASE_RECEIVER_ID,
            trusted_eligibility_id: eligibility_methods::NATIVE_ELIGIBILITY_ID,
            caller: Caller::Eligibility,
            fail_purchase: false,
            scratch_account_id: AccountId::for_regular_private_account(
                &NullifierPublicKey::from(&SCRATCH_SECRET_KEY),
                SCRATCH_IDENTIFIER,
            ),
        }
    }
}

// Guest execution alone does not authenticate caller provenance or native rollback.
pub struct ExecutionEvidence {
    pub accepted: bool,
    pub marker_used: bool,
    pub purchase_count: u64,
    pub collateral_in: u128,
    pub tokens_out: u128,
    pub program_runs: usize,
    pub user_cycles: u64,
    pub execute_seconds: f64,
    pub artifacts: crate::ArtifactEvidence,
}

pub(crate) fn programs() -> Result<ProgramWithDependencies> {
    let receiver = Program::new(Cow::Borrowed(
        eligibility_methods::NATIVE_PURCHASE_RECEIVER_ELF,
    ))
    .map_err(|_| opaque_error())?;
    let eligibility = Program::new(Cow::Borrowed(eligibility_methods::NATIVE_ELIGIBILITY_ELF))
        .map_err(|_| opaque_error())?;
    if receiver.id() != eligibility_methods::NATIVE_PURCHASE_RECEIVER_ID
        || eligibility.id() != eligibility_methods::NATIVE_ELIGIBILITY_ID
    {
        return Err(opaque_error());
    }
    Ok(ProgramWithDependencies::new(
        eligibility,
        [(receiver.id(), receiver)].into(),
    ))
}

pub(crate) fn initial_pre_states(trial: &Trial) -> Result<Vec<AccountWithMetadata>> {
    let config = TrustedConfig {
        eligibility_program_id: trial.trusted_eligibility_id,
        policy: trial.trusted_policy.clone(),
    };
    Ok(vec![
        AccountWithMetadata::new(
            Account {
                program_owner: trial.config_owner,
                data: encode(&config)
                    .ok_or_else(opaque_error)?
                    .try_into()
                    .map_err(|_| opaque_error())?,
                ..Account::default()
            },
            false,
            trial.config_account_id,
        ),
        AccountWithMetadata::new(trial.marker_account.clone(), false, trial.marker_account_id),
        AccountWithMetadata::new(
            trial.purchase_account.clone(),
            false,
            AccountId::new(trial.input.purchase.trading_account),
        ),
        AccountWithMetadata::new(Account::default(), true, trial.scratch_account_id),
    ])
}

pub(crate) fn root_instruction(trial: &Trial) -> Result<Vec<u32>> {
    Program::serialize_instruction(EligibilityInstruction {
        input: trial.input.clone(),
        receiver_purchase: trial.receiver_purchase,
        fail_purchase: trial.fail_purchase,
    })
    .map_err(|_| opaque_error())
}

fn opaque_error() -> eyre::Report {
    eyre::eyre!("native execution failed; witness details are not logged")
}

pub fn execute(trial: &Trial) -> Result<ExecutionEvidence> {
    crate::check_local_configuration()?;
    let segments = crate::protected_segments()?;
    let graph = programs()?;
    let pre_states = initial_pre_states(trial)?;
    let initial_state: MockPurchaseState = if trial.purchase_account.data.is_empty() {
        MockPurchaseState::default()
    } else {
        decode(&trial.purchase_account.data).ok_or_else(opaque_error)?
    };
    let mut marker_used = !trial.marker_account.data.is_empty();
    let mut state = initial_state.clone();
    let (initial_call, caller) = match trial.caller {
        Caller::Eligibility => (
            ChainedCall {
                program_id: graph.program.id(),
                pre_states,
                instruction_data: root_instruction(trial)?,
                pda_seeds: vec![],
            },
            None,
        ),
        Caller::Direct | Caller::Foreign => {
            let permit = check_eligibility(
                &trial.input.policy,
                &trial.input.purchase,
                &trial.input.witness,
            )
            .map_err(|_| opaque_error())?;
            let instruction = PurchaseInstruction {
                permit,
                purchase: trial.receiver_purchase,
                fail_purchase: trial.fail_purchase,
            };
            (
                ChainedCall::new(
                    eligibility_methods::NATIVE_PURCHASE_RECEIVER_ID,
                    vec![pre_states[0].clone(), pre_states[2].clone()],
                    &instruction,
                ),
                matches!(trial.caller, Caller::Foreign).then_some([0xf0; 8]),
            )
        }
    };
    let mut calls = VecDeque::from([(initial_call, caller)]);
    let mut program_runs = 0;
    let mut user_cycles = 0;
    let started = Instant::now();
    let mut accepted = true;
    while let Some((call, caller)) = calls.pop_front() {
        if program_runs >= MAX_NUMBER_CHAINED_CALLS {
            return Err(opaque_error());
        }
        let program = if call.program_id == graph.program.id() {
            &graph.program
        } else {
            graph
                .dependencies
                .get(&call.program_id)
                .ok_or_else(opaque_error)?
        };
        let mut builder = ExecutorEnv::builder();
        builder
            .segment_path(segments.path())
            .session_limit(Some(PUBLIC_EXECUTION_CAP))
            .segment_limit_po2(20);
        // Pinned Program::write_inputs is private; preserve its native wire order.
        builder.write(&program.id()).map_err(|_| opaque_error())?;
        builder.write(&caller).map_err(|_| opaque_error())?;
        builder
            .write(&call.pre_states)
            .map_err(|_| opaque_error())?;
        builder
            .write(&call.instruction_data)
            .map_err(|_| opaque_error())?;
        let env = builder.build().map_err(|_| opaque_error())?;
        let server = std::env::var_os("RISC0_SERVER_PATH").unwrap_or_else(|| "r0vm".into());
        program_runs += 1;
        let info = ExternalProver::new("local-ipc", server)
            .execute(env, program.elf())
            .map_err(|_| opaque_error())?;
        user_cycles += info.cycles();
        if info.exit_code != risc0_zkvm::ExitCode::Halted(0) {
            accepted = false;
            break;
        }
        let output: ProgramOutput = info.journal.decode().map_err(|_| opaque_error())?;
        if output.self_program_id != program.id()
            || output.caller_program_id != caller
            || output.instruction_data != call.instruction_data
            || output.pre_states != call.pre_states
        {
            return Err(opaque_error());
        }
        lee_core::program::validate_execution(
            &output.pre_states,
            &output.post_states,
            program.id(),
        )
        .map_err(|_| opaque_error())?;
        if program.id() == graph.program.id() {
            marker_used = output.post_states[1].account().data.as_ref() == [1];
        } else {
            state = decode(&output.post_states[1].account().data).ok_or_else(opaque_error)?;
        }
        for chained in output.chained_calls {
            calls.push_back((chained, Some(program.id())));
        }
    }
    if !accepted {
        marker_used = !trial.marker_account.data.is_empty();
        state = initial_state;
    }
    Ok(ExecutionEvidence {
        accepted,
        marker_used,
        purchase_count: state.count,
        collateral_in: state.collateral_in,
        tokens_out: state.tokens_out,
        program_runs,
        user_cycles,
        execute_seconds: started.elapsed().as_secs_f64(),
        artifacts: crate::finish_segments(segments)?,
    })
}
