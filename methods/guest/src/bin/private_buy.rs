//! One private transaction: native funding + collateral deshield + buy + re-shield.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

use {factory_core as _, risc0_zkvm as _};

use associated_token_account_core::Instruction as AtaInstruction;
use curve_core::Instruction as CurveInstruction;
use lee_core::{
    account::{AccountWithMetadata, Data},
    program::{AccountPostState, ChainedCall, ProgramInput, ProgramOutput, read_lee_inputs},
};
use private_flow_core::PrivateBuyInstruction;
use token_core::TokenHolding;

fn main() {
    let (
        ProgramInput {
            self_program_id,
            caller_program_id,
            pre_states,
            instruction,
        },
        instruction_words,
    ) = read_lee_inputs::<PrivateBuyInstruction>();

    let PrivateBuyInstruction::Execute {
        curve_program_id,
        token_program_id,
        ata_program_id,
        native_transfer_program_id,
        amount_out,
        max_collateral_in,
        gas_reserve,
        collateral_definition_id,
    } = instruction;
    let [
        private_source,
        transient,
        collateral_definition,
        launch_definition,
        transient_collateral_ata,
        transient_token_ata,
        pool,
        config,
        pool_collateral_ata,
        pool_token_ata,
        clock,
        private_destination,
    ] = pre_states
        .clone()
        .try_into()
        .expect("PrivateBuy requires exactly twelve accounts");

    assert!(gas_reserve > 0, "PrivateBuy requires native gas");
    assert!(max_collateral_in > 0, "PrivateBuy requires collateral");
    assert_eq!(
        collateral_definition.account_id, collateral_definition_id,
        "Collateral definition does not match instruction"
    );
    assert_eq!(
        private_source.account.program_owner, token_program_id,
        "Private source must be a token holding"
    );

    let mut source_authorized = private_source.clone();
    source_authorized.is_authorized = true;
    let mut transient_authorized = transient.clone();
    transient_authorized.is_authorized = true;

    let mut source_after_gas = source_authorized.clone();
    source_after_gas.account.balance = source_after_gas
        .account
        .balance
        .checked_sub(gas_reserve)
        .expect("Private source has insufficient native balance");
    let mut transient_after_gas = transient_authorized.clone();
    transient_after_gas.account.balance = transient_after_gas
        .account
        .balance
        .checked_add(gas_reserve)
        .expect("Transient native balance overflow");
    transient_after_gas.account.program_owner = native_transfer_program_id;

    let initialized_ata = |mut account: AccountWithMetadata, definition_id| {
        account.account.program_owner = token_program_id;
        account.account.data = Data::from(&TokenHolding::Fungible {
            definition_id,
            balance: 0,
        });
        account
    };
    let transient_collateral_ata_initialized =
        initialized_ata(transient_collateral_ata.clone(), collateral_definition_id);
    let transient_token_ata_initialized =
        initialized_ata(transient_token_ata.clone(), launch_definition.account_id);
    let mut transient_collateral_ata_funded = transient_collateral_ata_initialized.clone();
    transient_collateral_ata_funded.account.data = Data::from(&TokenHolding::Fungible {
        definition_id: collateral_definition_id,
        balance: max_collateral_in,
    });
    let mut transient_token_ata_after_swap = transient_token_ata_initialized.clone();
    transient_token_ata_after_swap.account.data = Data::from(&TokenHolding::Fungible {
        definition_id: launch_definition.account_id,
        balance: amount_out,
    });

    let calls = vec![
        ChainedCall::new(
            native_transfer_program_id,
            vec![source_authorized.clone(), transient_authorized.clone()],
            &authenticated_transfer_core::Instruction::Transfer {
                amount: gas_reserve,
            },
        ),
        ChainedCall::new(
            ata_program_id,
            vec![
                transient_after_gas.clone(),
                collateral_definition.clone(),
                transient_collateral_ata.clone(),
            ],
            &AtaInstruction::Create { ata_program_id },
        ),
        ChainedCall::new(
            ata_program_id,
            vec![
                transient_after_gas.clone(),
                launch_definition.clone(),
                transient_token_ata.clone(),
            ],
            &AtaInstruction::Create { ata_program_id },
        ),
        ChainedCall::new(
            token_program_id,
            vec![
                source_after_gas,
                transient_collateral_ata_initialized.clone(),
            ],
            &token_core::Instruction::Transfer {
                amount_to_transfer: max_collateral_in,
            },
        ),
        ChainedCall::new(
            curve_program_id,
            vec![
                pool,
                config,
                transient_after_gas.clone(),
                transient_collateral_ata_funded,
                pool_collateral_ata,
                pool_token_ata,
                transient_token_ata_initialized,
                clock,
            ],
            &CurveInstruction::Private {
                instruction: Box::new(CurveInstruction::SwapExactOutput {
                    amount_out,
                    max_amount_in: max_collateral_in,
                    token_in: collateral_definition_id,
                }),
            },
        ),
        ChainedCall::new(
            ata_program_id,
            vec![
                transient_after_gas,
                transient_token_ata_after_swap,
                private_destination,
            ],
            &AtaInstruction::Transfer {
                ata_program_id,
                amount: amount_out,
            },
        ),
    ];

    let post_states = pre_states
        .iter()
        .map(|state| AccountPostState::new(state.account.clone()))
        .collect();
    ProgramOutput::new(
        self_program_id,
        caller_program_id,
        instruction_words,
        pre_states,
        post_states,
    )
    .with_chained_calls(calls)
    .write();
}
