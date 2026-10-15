//! Test-only complete setup graph. Never use this guest as a launch factory.

use factory_core::{
    CreatorAllocationRelease, FixedPriceCreationStage, FixedPriceLaunchState, Instruction,
};
use lee_core::{
    account::{AccountId, AccountWithMetadata, Data},
    program::{ChainedCall, PdaSeed, ProgramOutput, read_lee_inputs},
};
use token_core::{TokenDefinition, TokenHolding};

fn holding(
    mut account: AccountWithMetadata,
    definition: AccountId,
    balance: u128,
) -> AccountWithMetadata {
    account.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    account.account.data = Data::from(&TokenHolding::Fungible {
        definition_id: definition,
        balance,
    });
    account.is_authorized = false;
    account
}
fn create_ata(
    owner: AccountWithMetadata,
    definition: AccountWithMetadata,
    ata: AccountWithMetadata,
) -> ChainedCall {
    ChainedCall::new(
        curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        vec![owner, definition, ata],
        &associated_token_account_core::Instruction::Create {
            ata_program_id: curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        },
    )
}
fn transfer(
    mut source: AccountWithMetadata,
    recipient: AccountWithMetadata,
    amount: u128,
    seeds: Vec<PdaSeed>,
) -> ChainedCall {
    source.is_authorized = true;
    ChainedCall::new(
        curve_core::TOKEN_PROGRAM_ID,
        vec![source, recipient],
        &token_core::Instruction::Transfer {
            amount_to_transfer: amount,
        },
    )
    .with_pda_seeds(seeds)
}
fn main() {
    let (input, words) = read_lee_inputs::<Instruction>();
    let Instruction::CreateFixedPriceLbp { inputs } = input.instruction else {
        panic!("Probe accepts only fixed-price creation");
    };
    let pre = input.pre_states;
    let (mut posts, mut calls) = factory_core::process_instruction(
        pre.clone(),
        Instruction::CreateFixedPriceLbp {
            inputs: inputs.clone(),
        },
        input.self_program_id,
    );
    let mut state = FixedPriceLaunchState::try_from(&posts[0].account().data).unwrap();
    state.creation_stage = FixedPriceCreationStage::Complete;
    state.creator_allocation_released = inputs.creator_allocation == 0
        || inputs.allocation_release == CreatorAllocationRelease::AfterSetup;
    posts[0].account_mut().data = Data::from(&state);
    let mut owner = pre[0].clone();
    owner.account = posts[0].account().clone();
    owner.account.program_owner = input.self_program_id;
    owner.is_authorized = false;
    let mut definition = pre[1].clone();
    definition.account.program_owner = curve_core::TOKEN_PROGRAM_ID;
    definition.account.data = Data::from(&TokenDefinition::Fungible {
        name: inputs.name.clone(),
        total_supply: state.total_supply,
        metadata_id: Some(pre[3].account_id),
    });
    let mint = holding(pre[2].clone(), definition.account_id, state.total_supply);
    let factory_ata = holding(pre[8].clone(), definition.account_id, 0);
    calls.push(create_ata(
        owner.clone(),
        definition.clone(),
        pre[8].clone(),
    ));
    calls.push(transfer(
        mint.clone(),
        factory_ata,
        inputs.sale_inventory,
        vec![factory_core::compute_mint_seed(
            inputs.namespace,
            inputs.launch_salt,
        )],
    ));
    let mint = holding(mint, definition.account_id, inputs.creator_allocation);
    if inputs.creator_allocation > 0 {
        let mut escrow = pre[4].clone();
        escrow.is_authorized = true;
        calls.push(transfer(
            mint,
            escrow,
            inputs.creator_allocation,
            vec![
                factory_core::compute_mint_seed(inputs.namespace, inputs.launch_salt),
                factory_core::compute_escrow_seed(inputs.namespace, inputs.launch_salt),
            ],
        ));
    }
    let factory_seed = factory_core::compute_factory_seed(inputs.namespace, inputs.launch_salt);
    owner.is_authorized = true;
    let pool_inputs = lbp_core::FixedPricePoolInputs {
        namespace: inputs.namespace,
        namespace_program_id: inputs.namespace_program_id,
        factory_program_id: input.self_program_id,
        factory_seed: *factory_seed.as_bytes(),
        sale_inventory: inputs.sale_inventory,
        creator_allocation: inputs.creator_allocation,
        price: inputs.price,
        start_timestamp: state.start_timestamp,
        end_timestamp: state.end_timestamp,
        creator_allocation_release: inputs.allocation_release,
    };
    let pool_accounts = vec![
        pre[9].clone(),
        owner.clone(),
        definition.clone(),
        pre[7].clone(),
        holding(pre[8].clone(), definition.account_id, inputs.sale_inventory),
        pre[10].clone(),
        pre[11].clone(),
        pre[12].clone(),
        pre[13].clone(),
    ];
    calls.push(
        ChainedCall::new(
            inputs.lbp_program_id,
            pool_accounts.clone(),
            &lbp_core::Instruction::PrepareFixedPrice {
                inputs: pool_inputs.clone(),
            },
        )
        .with_pda_seeds(vec![factory_seed]),
    );
    let pending = lbp_core::PoolAccount {
        mode: lbp_core::PoolMode::FixedPrice,
        funded: false,
        namespace: pool_inputs.namespace,
        namespace_program_id: pool_inputs.namespace_program_id,
        factory_owner: owner.account_id,
        factory_program_id: pool_inputs.factory_program_id,
        factory_seed: pool_inputs.factory_seed,
        token_definition_id: definition.account_id,
        collateral_definition_id: pre[7].account_id,
        sale_inventory: pool_inputs.sale_inventory,
        remaining_inventory: pool_inputs.sale_inventory,
        creator_allocation: pool_inputs.creator_allocation,
        price: pool_inputs.price,
        start_timestamp: pool_inputs.start_timestamp,
        end_timestamp: pool_inputs.end_timestamp,
        creator_allocation_release: pool_inputs.creator_allocation_release,
        recorded_collateral: 0,
        accrued_fees: 0,
    };
    let mut activation_accounts = pool_accounts;
    activation_accounts[0].account.program_owner = inputs.lbp_program_id;
    activation_accounts[0].account.data = Data::from(&pending);
    activation_accounts[5] = holding(pre[10].clone(), definition.account_id, 0);
    activation_accounts[6] = holding(pre[11].clone(), pre[7].account_id, 0);
    calls.push(
        ChainedCall::new(
            inputs.lbp_program_id,
            activation_accounts,
            &lbp_core::Instruction::ActivateFixedPrice,
        )
        .with_pda_seeds(vec![factory_seed]),
    );
    if inputs.creator_allocation > 0
        && inputs.allocation_release == CreatorAllocationRelease::AfterSetup
    {
        calls.push(create_ata(
            pre[5].clone(),
            definition.clone(),
            pre[6].clone(),
        ));
        calls.push(transfer(
            holding(
                pre[4].clone(),
                definition.account_id,
                inputs.creator_allocation,
            ),
            holding(pre[6].clone(), definition.account_id, 0),
            inputs.creator_allocation,
            vec![factory_core::compute_escrow_seed(
                inputs.namespace,
                inputs.launch_salt,
            )],
        ));
    }
    ProgramOutput::new(
        input.self_program_id,
        input.caller_program_id,
        words,
        pre,
        posts,
    )
    .with_chained_calls(calls)
    .write();
}
