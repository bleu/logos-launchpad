//! Test adapter for the public graduation interface.

use factory_core::{FactoryState, GraduationRequest, SettlementStage};
use lee_core::{
    account::{AccountId, Data},
    program::{AccountPostState, Claim, PdaSeed, ProgramOutput, read_lee_inputs},
};

fn main() {
    let (input, words) = read_lee_inputs::<GraduationRequest>();
    let request = &input.instruction;
    let factory = factory_core::compute_factory_pda(
        request.namespace,
        request.factory_program_id,
        request.launch_salt,
    );
    let readiness_seed = PdaSeed::new(factory.to_bytes());
    let readiness = AccountId::for_public_pda(&input.self_program_id, &readiness_seed);
    let mut post: Vec<_> = input
        .pre_states
        .iter()
        .map(|pre| AccountPostState::new(pre.account.clone()))
        .collect();
    if input.caller_program_id.is_none()
        && (request.configuration == b"ready" || request.configuration == b"not-ready")
    {
        assert_eq!(input.pre_states.len(), 2);
        assert_eq!(input.pre_states[0].account_id, readiness);
        let mut account = input.pre_states[0].account.clone();
        account.data = Data::try_from(vec![u8::from(request.configuration == b"ready")]).unwrap();
        post[0] = AccountPostState::new_claimed_if_default(account, Claim::Pda(readiness_seed));
        assert_eq!(
            input.pre_states[1].account_id,
            factory_core::compute_graduation_pda(input.self_program_id, factory)
        );
        post[1] = AccountPostState::new_claimed_if_default(
            input.pre_states[1].account.clone(),
            Claim::Pda(factory_core::compute_graduation_seed(factory)),
        );
    } else {
        assert_eq!(input.caller_program_id, Some(request.factory_program_id));
        assert_eq!(input.pre_states.len(), 7);
        assert!(input.pre_states.iter().all(|pre| !pre.is_authorized));
        assert_eq!(input.pre_states[0].account_id, factory);
        assert_eq!(
            input.pre_states[0].account.program_owner,
            request.factory_program_id
        );
        let state = FactoryState::try_from(&input.pre_states[0].account.data).unwrap();
        assert_eq!(state.settlement_stage, SettlementStage::Graduated);
        let hook = state.graduation_hook.as_ref().unwrap();
        assert_eq!(hook.program_id, input.self_program_id);
        assert_eq!(hook.configuration, request.configuration);
        assert_eq!(state.dex_seed_reserve, request.dex_seed_reserve);
        assert_eq!(state.token_definition_id, request.token_definition_id);
        assert_eq!(
            state.collateral_definition_id,
            request.collateral_definition_id
        );
        assert_eq!(input.pre_states[2].account_id, request.token_definition_id);
        assert_eq!(
            input.pre_states[3].account_id,
            request.collateral_definition_id
        );
        let custody = factory_core::compute_graduation_pda(input.self_program_id, factory);
        assert_eq!(input.pre_states[1].account_id, custody);
        for (slot, definition, amount) in [
            (4, request.token_definition_id, request.dex_seed_reserve),
            (
                5,
                request.collateral_definition_id,
                request.collateral_amount,
            ),
        ] {
            associated_token_account_core::verify_ata_and_get_seed(
                &input.pre_states[slot],
                &input.pre_states[1],
                definition,
                curve_core::ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
            );
            match token_core::TokenHolding::try_from(&input.pre_states[slot].account.data).unwrap()
            {
                token_core::TokenHolding::Fungible {
                    definition_id,
                    balance,
                } => {
                    assert_eq!(definition_id, definition);
                    assert!(balance >= amount);
                }
                _ => panic!("fungible adapter custody"),
            }
        }
        assert_eq!(input.pre_states[6].account_id, readiness);
        assert_eq!(
            input.pre_states[6].account.program_owner,
            input.self_program_id
        );
        assert_eq!(
            input.pre_states[6].account.data.as_ref(),
            &[1],
            "Adapter is not ready"
        );
        let mut receipt = request.dex_seed_reserve.to_le_bytes().to_vec();
        receipt.extend(request.collateral_amount.to_le_bytes());
        receipt.extend(&request.configuration);
        let mut account = input.pre_states[1].account.clone();
        account.data = Data::try_from(receipt).unwrap();
        post[1] = AccountPostState::new_claimed_if_default(
            account,
            Claim::Pda(factory_core::compute_graduation_seed(factory)),
        );
    }
    ProgramOutput::new(
        input.self_program_id,
        input.caller_program_id,
        words,
        input.pre_states,
        post,
    )
    .write();
}
