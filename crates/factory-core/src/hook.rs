use super::*;

pub(super) fn validate_interface(
    pre: &[AccountWithMetadata],
    remaining: &[AccountWithMetadata],
    program: ProgramId,
) {
    for slot in [0, 2, 3, 4, 9, 10] {
        assert!(
            !pre[slot].is_authorized,
            "Hook callback prefix must be unauthorized"
        );
    }
    let state = FactoryState::try_from(&pre[0].account.data).expect("valid factory");
    let mut protected: Vec<_> = pre.iter().map(|account| account.account_id).collect();
    protected.extend([
        state.creator_escrow_id,
        compute_escrow_pda(state.namespace, program, state.launch_salt),
        compute_definition_pda(state.namespace, program, state.launch_salt),
        compute_mint_pda(state.namespace, program, state.launch_salt),
        compute_metadata_pda(state.namespace, program, state.launch_salt),
    ]);
    for account in remaining {
        assert!(
            !account.is_authorized,
            "Hook remaining accounts must be unauthorized"
        );
        assert_ne!(
            account.account.program_owner, program,
            "Hook cannot expose factory-owned accounts"
        );
        assert!(
            !protected.contains(&account.account_id),
            "Hook remaining account aliases protected or duplicate account"
        );
        protected.push(account.account_id);
    }
}

pub(super) fn validate_settlement(pre: &[AccountWithMetadata], program: ProgramId) {
    let state = FactoryState::try_from(&pre[0].account.data).expect("valid factory");
    let pool = PoolAccount::try_from(&pre[1].account.data).expect("valid pool");
    assert_eq!(
        pre[1].account_id,
        compute_pool_pda(
            state.namespace,
            state.curve_program_id,
            state.token_definition_id,
            state.collateral_definition_id,
            pre[0].account_id
        ),
        "Wrong canonical settlement pool"
    );
    assert_eq!(pool.owner, pre[0].account_id, "Wrong settlement pool owner");
    assert_eq!(
        pool.owner_program,
        Some((
            program,
            *compute_factory_seed(state.namespace, state.launch_salt).as_bytes()
        )),
        "Wrong settlement pool authority"
    );
    assert_eq!(
        pool.token0_definition_id, state.token_definition_id,
        "Wrong pool launch definition"
    );
    assert_eq!(
        pool.token1_definition_id, state.collateral_definition_id,
        "Wrong pool collateral definition"
    );
    let config = curve_core::validated_config(&pre[12], state.namespace, state.curve_program_id);
    let treasury = AccountWithMetadata {
        account_id: config.treasury,
        account: Account::default(),
        is_authorized: false,
    };
    for (slot, owner, token) in [
        (7, &pre[1], state.token_definition_id),
        (8, &pre[1], state.collateral_definition_id),
        (13, &treasury, state.collateral_definition_id),
    ] {
        associated_token_account_core::verify_ata_and_get_seed(
            &pre[slot],
            owner,
            token,
            ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID,
        );
    }
    for slot in [3, 4] {
        assert_eq!(
            pre[slot].account.program_owner,
            curve_core::TOKEN_PROGRAM_ID,
            "Invalid settlement definition program"
        );
        assert!(
            matches!(
                token_core::TokenDefinition::try_from(&pre[slot].account.data)
                    .expect("valid settlement definition"),
                token_core::TokenDefinition::Fungible { .. }
            ),
            "Settlement definitions must be fungible"
        );
    }
    for (slot, token) in [
        (5, state.token_definition_id),
        (6, state.collateral_definition_id),
        (7, state.token_definition_id),
        (8, state.collateral_definition_id),
        (9, state.token_definition_id),
        (10, state.collateral_definition_id),
        (13, state.collateral_definition_id),
    ] {
        if pre[slot].account != Account::default() {
            balance(&pre[slot], token);
        }
    }
    match state.settlement_stage {
        SettlementStage::Ready => {
            assert!(
                balance(&pre[7], state.token_definition_id) >= pool.pool.real_reserve0,
                "Pool token custody is underfunded"
            );
            let collateral_liability = pool
                .pool
                .real_reserve1
                .checked_add(pool.pool.fees_accrued)
                .expect("Pool collateral liabilities overflow");
            assert!(
                balance(&pre[8], state.collateral_definition_id) >= collateral_liability,
                "Pool collateral custody is underfunded"
            );
            let treasury_credit = if pre[13].account_id == pre[8].account_id {
                0
            } else {
                pool.pool.fees_accrued
            };
            check_credits(
                pre,
                &[
                    (5, state.token_definition_id, pool.pool.real_reserve0),
                    (6, state.collateral_definition_id, pool.pool.real_reserve1),
                    (13, state.collateral_definition_id, treasury_credit),
                ],
            );
        }
        SettlementStage::Withdrawn { unsold, .. } if unsold != 0 => {
            assert!(
                balance(&pre[5], state.token_definition_id) >= unsold,
                "Unsold allocation custody is underfunded"
            );
            let token_core::TokenDefinition::Fungible { total_supply, .. } =
                token_core::TokenDefinition::try_from(&pre[3].account.data)
                    .expect("valid definition")
            else {
                unreachable!()
            };
            assert!(total_supply >= unsold, "Unsold burn exceeds total supply");
        }
        SettlementStage::Burned { .. } | SettlementStage::HookReady { .. }
            if state.dex_seed_reserve != 0 =>
        {
            assert!(
                balance(&pre[5], state.token_definition_id) >= state.dex_seed_reserve,
                "Factory token custody is underfunded"
            );
            check_credits(
                pre,
                &[(9, state.token_definition_id, state.dex_seed_reserve)],
            );
        }
        SettlementStage::Burned { collateral_due }
        | SettlementStage::HookReady { collateral_due }
        | SettlementStage::TokenPaid { collateral_due } => {
            assert!(
                balance(&pre[6], state.collateral_definition_id) >= collateral_due,
                "Factory collateral custody is underfunded"
            );
            check_credits(pre, &[(10, state.collateral_definition_id, collateral_due)]);
        }
        _ => {}
    }
}

fn check_credits(pre: &[AccountWithMetadata], credits: &[(usize, AccountId, u128)]) {
    let mut totals: Vec<(AccountId, u128)> = Vec::new();
    for &(slot, token, amount) in credits {
        let id = pre[slot].account_id;
        if let Some((_, value)) = totals.iter_mut().find(|(account, _)| *account == id) {
            *value = value
                .checked_add(amount)
                .expect("Settlement custody credit overflow");
        } else {
            let value = if pre[slot].account == Account::default() {
                0
            } else {
                balance(&pre[slot], token)
            };
            totals.push((
                id,
                value
                    .checked_add(amount)
                    .expect("Settlement custody credit overflow"),
            ));
        }
    }
}

fn balance(account: &AccountWithMetadata, definition: AccountId) -> u128 {
    assert_eq!(
        account.account.program_owner,
        curve_core::TOKEN_PROGRAM_ID,
        "Invalid settlement token program"
    );
    match TokenHolding::try_from(&account.account.data).expect("valid settlement token holding") {
        TokenHolding::Fungible {
            definition_id,
            balance,
        } => {
            assert_eq!(
                definition_id, definition,
                "Wrong settlement holding definition"
            );
            balance
        }
        _ => panic!("Settlement tokens must be fungible"),
    }
}

fn credited(
    account: &AccountWithMetadata,
    definition: AccountId,
    amount: u128,
) -> AccountWithMetadata {
    let value = balance(account, definition)
        .checked_add(amount)
        .expect("Adapter custody balance overflow");
    let mut snapshot = account.clone();
    snapshot.account.data = Data::from(&TokenHolding::Fungible {
        definition_id: definition,
        balance: value,
    });
    snapshot
}

pub(super) fn final_calls(
    pre: &[AccountWithMetadata],
    snapshot: AccountWithMetadata,
    state: &FactoryState,
    factory_program_id: ProgramId,
    collateral_due: u128,
) -> Vec<ChainedCall> {
    let settings = state.graduation_hook.as_ref().expect("configured hook");
    assert!(
        balance(&pre[5], state.token_definition_id) >= state.dex_seed_reserve,
        "Factory token custody is underfunded"
    );
    assert!(
        balance(&pre[6], state.collateral_definition_id) >= collateral_due,
        "Factory collateral custody is underfunded"
    );
    let token = credited(&pre[9], state.token_definition_id, state.dex_seed_reserve);
    let collateral = credited(&pre[10], state.collateral_definition_id, collateral_due);
    let owner = AccountWithMetadata {
        is_authorized: true,
        ..snapshot.clone()
    };
    let mut calls = Vec::new();
    for (source, recipient, definition, amount) in [
        (&pre[5], &pre[9], &pre[3], state.dex_seed_reserve),
        (&pre[6], &pre[10], &pre[4], collateral_due),
    ] {
        if amount != 0 {
            let mut transfer = payout_calls(
                owner.clone(),
                pre[2].clone(),
                source.clone(),
                recipient.clone(),
                definition.clone(),
                amount,
                vec![compute_factory_seed(state.namespace, state.launch_salt)],
            );
            transfer.remove(0);
            calls.extend(transfer);
        }
    }
    calls.push(ChainedCall::new(
        settings.program_id,
        vec![
            snapshot,
            pre[2].clone(),
            pre[3].clone(),
            pre[4].clone(),
            token,
            collateral,
        ],
        &GraduationRequest {
            factory_program_id,
            namespace: state.namespace,
            launch_salt: state.launch_salt,
            token_definition_id: state.token_definition_id,
            collateral_definition_id: state.collateral_definition_id,
            dex_seed_reserve: state.dex_seed_reserve,
            collateral_amount: collateral_due,
            configuration: settings.configuration.clone(),
        },
    ));
    calls
}
