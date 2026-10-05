use crate::{
    EligibilityInput, EligibilityPermit, Purchase, SalePolicy, check_eligibility,
    check_public_binding,
};
use lee_core::{
    account::AccountId,
    program::{
        AccountPostState, ChainedCall, Claim, PdaSeed, ProgramId, ProgramInput, ProgramOutput,
    },
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const CONFIG_ACCOUNT_ID: AccountId = AccountId::new([0xc1; 32]);

#[derive(Clone, Serialize, Deserialize)]
pub struct TrustedConfig {
    pub eligibility_program_id: ProgramId,
    pub policy: SalePolicy,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct EligibilityInstruction {
    pub input: EligibilityInput,
    pub receiver_purchase: Purchase,
    pub fail_purchase: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PurchaseInstruction {
    pub permit: EligibilityPermit,
    pub purchase: Purchase,
    pub fail_purchase: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct MockPurchaseState {
    pub count: u64,
    pub collateral_in: u128,
    pub tokens_out: u128,
}

pub fn encode<T: Serialize>(value: &T) -> Option<Vec<u8>> {
    Some(
        risc0_zkvm::serde::to_vec(value)
            .ok()?
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect(),
    )
}

pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Option<T> {
    if !bytes.len().is_multiple_of(4) {
        return None;
    }
    let words: Vec<u32> = bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
        .collect();
    risc0_zkvm::serde::from_slice(&words).ok()
}

pub fn eligibility(
    input: ProgramInput<EligibilityInstruction>,
    words: Vec<u32>,
) -> Option<ProgramOutput> {
    let [config, marker, purchase, scratch] = input.pre_states.as_slice() else {
        return None;
    };
    let permit = check_eligibility(
        &input.instruction.input.policy,
        &input.instruction.input.purchase,
        &input.instruction.input.witness,
    )
    .ok()?;
    let seed = PdaSeed::new(permit.nullifier);
    if marker.account != lee_core::account::Account::default()
        || marker.account_id != AccountId::for_public_pda(&input.self_program_id, &seed)
    {
        return None;
    }
    let mut used_marker = marker.account.clone();
    used_marker.data = vec![1].try_into().ok()?;
    let post_states = vec![
        AccountPostState::new(config.account.clone()),
        AccountPostState::new_claimed(used_marker, Claim::Pda(seed)),
        AccountPostState::new(purchase.account.clone()),
        AccountPostState::new_claimed(scratch.account.clone(), Claim::Authorized),
    ];
    let call = ChainedCall::new(
        config.account.program_owner,
        vec![config.clone(), purchase.clone()],
        &PurchaseInstruction {
            permit,
            purchase: input.instruction.receiver_purchase,
            fail_purchase: input.instruction.fail_purchase,
        },
    );
    Some(
        ProgramOutput::new(
            input.self_program_id,
            input.caller_program_id,
            words,
            input.pre_states,
            post_states,
        )
        .with_chained_calls(vec![call]),
    )
}

pub fn purchase(
    input: ProgramInput<PurchaseInstruction>,
    words: Vec<u32>,
) -> Option<ProgramOutput> {
    if input.instruction.fail_purchase {
        return None;
    }
    let [config, purchase] = input.pre_states.as_slice() else {
        return None;
    };
    if config.account_id != CONFIG_ACCOUNT_ID
        || config.account.program_owner != input.self_program_id
    {
        return None;
    }
    let trusted: TrustedConfig = decode(&config.account.data)?;
    if input.caller_program_id != Some(trusted.eligibility_program_id) {
        return None;
    }
    check_public_binding(
        &trusted.policy,
        &input.instruction.purchase,
        &input.instruction.permit,
    )
    .ok()?;
    if purchase.account_id != AccountId::new(input.instruction.purchase.trading_account)
        || purchase.account.program_owner != input.self_program_id
    {
        return None;
    }
    let mut state = if purchase.account.data.is_empty() {
        MockPurchaseState::default()
    } else {
        decode(&purchase.account.data)?
    };
    state.count = state.count.checked_add(1)?;
    state.collateral_in = state
        .collateral_in
        .checked_add(input.instruction.purchase.collateral_in)?;
    state.tokens_out = state
        .tokens_out
        .checked_add(input.instruction.purchase.tokens_out)?;
    let mut post_purchase = purchase.account.clone();
    post_purchase.data = encode(&state)?.try_into().ok()?;
    let post_states = vec![
        AccountPostState::new(config.account.clone()),
        AccountPostState::new(post_purchase),
    ];
    Some(ProgramOutput::new(
        input.self_program_id,
        input.caller_program_id,
        words,
        input.pre_states,
        post_states,
    ))
}
