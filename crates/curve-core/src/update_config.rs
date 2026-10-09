//! Namespace administration through the library's public account API.

use admin_authority::{AdminCandidate, AdminConfig};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata, Data},
    program::{AccountPostState, Claim, ProgramId},
};
use spel_framework::prelude as spel;

use crate::{Config, MAX_FEE_BPS, compute_config_pda, compute_config_pda_seed};

// The library and root use distinct LEZ crate identities with the same public fields.
pub(crate) fn adapt(source: &AccountWithMetadata) -> spel::AccountWithMetadata {
    spel::AccountWithMetadata {
        account_id: spel::AccountId::new(source.account_id.to_bytes()),
        is_authorized: source.is_authorized,
        account: spel::Account {
            program_owner: source.account.program_owner,
            balance: source.account.balance,
            data: source
                .account
                .data
                .to_vec()
                .try_into()
                .expect("Library account data limit"),
            nonce: source.account.nonce.0.into(),
        },
    }
}

pub fn initialize_namespace(
    namespace: AccountId,
    config: AccountWithMetadata,
    creator: AccountWithMetadata,
    protocol_fee_bps: u32,
    treasury: AccountId,
    program: ProgramId,
) -> Vec<AccountPostState> {
    assert_eq!(
        config.account_id,
        compute_config_pda(namespace, program),
        "Config account ID does not match PDA"
    );
    assert_eq!(
        config.account,
        Account::default(),
        "Namespace config already exists"
    );
    assert_ne!(
        namespace,
        AccountId::default(),
        "Namespace must not be the default key"
    );
    assert_eq!(
        creator.account_id, namespace,
        "Creator is not the namespace account"
    );
    assert!(
        protocol_fee_bps < u32::from(MAX_FEE_BPS),
        "Protocol fee must be below 10,000 basis points"
    );
    assert_ne!(
        treasury,
        AccountId::default(),
        "Treasury must not be the default key"
    );
    let state = Config {
        protocol_fee_bps: protocol_fee_bps as u16,
        treasury,
        ..Config::default()
    };
    let mut embedded = adapt(&config);
    embedded.account.data = Data::from(&state)
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    AdminConfig::bootstrap_at(
        &mut embedded,
        Config::ADMIN_OFFSET,
        AdminCandidate::Signer,
        &adapt(&creator),
    )
    .expect("Namespace creator authorization failed");
    let mut post = config.account;
    post.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    vec![
        AccountPostState::new_claimed_if_default(
            post,
            Claim::Pda(compute_config_pda_seed(namespace)),
        ),
        AccountPostState::new(creator.account),
    ]
}

pub fn update_fee(
    namespace: AccountId,
    config: AccountWithMetadata,
    admin: AccountWithMetadata,
    fee: u32,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let mut state = crate::validated_config(&config, namespace, program);
    state.assert_admin(&admin);
    assert!(
        fee < u32::from(MAX_FEE_BPS),
        "Protocol fee must be below 10,000 basis points"
    );
    state.protocol_fee_bps = fee as u16;
    let mut post = config.account;
    post.data = (&state).into();
    vec![
        AccountPostState::new(post),
        AccountPostState::new(admin.account),
    ]
}

pub fn update_treasury(
    namespace: AccountId,
    config: AccountWithMetadata,
    admin: AccountWithMetadata,
    treasury: AccountId,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let mut state = crate::validated_config(&config, namespace, program);
    state.assert_admin(&admin);
    assert_ne!(
        treasury,
        AccountId::default(),
        "Treasury must not be the default key"
    );
    state.treasury = treasury;
    let mut post = config.account;
    post.data = (&state).into();
    vec![
        AccountPostState::new(post),
        AccountPostState::new(admin.account),
    ]
}

pub fn transfer_admin(
    pre: Vec<AccountWithMetadata>,
    namespace: AccountId,
    new_admin: AccountId,
    candidate: crate::AdminCandidate,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let [config, current, successor] = pre
        .try_into()
        .expect("TransferAdmin requires exactly three accounts");
    crate::validated_config(&config, namespace, program);
    assert_eq!(
        successor.account_id, new_admin,
        "Successor account ID does not match new admin"
    );
    let candidate = match candidate {
        crate::AdminCandidate::Signer => AdminCandidate::Signer,
        crate::AdminCandidate::Pda { program_id, seed } => {
            assert_eq!(
                successor.account.program_owner, program_id,
                "PDA administrator belongs to another program"
            );
            AdminCandidate::Pda { program_id, seed }
        }
    };
    let mut embedded = adapt(&config);
    AdminConfig::perform_transfer_at(
        &mut embedded,
        Config::ADMIN_OFFSET,
        &adapt(&current),
        candidate,
        &adapt(&successor),
    )
    .expect("Namespace administration transfer failed");
    let mut post = config.account;
    post.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    vec![
        AccountPostState::new(post),
        AccountPostState::new(current.account),
        AccountPostState::new(successor.account),
    ]
}

pub fn renounce_admin(
    namespace: AccountId,
    config: AccountWithMetadata,
    authority: AccountWithMetadata,
    program: ProgramId,
) -> Vec<AccountPostState> {
    crate::validated_config(&config, namespace, program);
    let mut embedded = adapt(&config);
    AdminConfig::perform_renounce_at(&mut embedded, Config::ADMIN_OFFSET, &adapt(&authority))
        .expect("Namespace renunciation failed");
    let mut post = config.account;
    post.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    vec![
        AccountPostState::new(post),
        AccountPostState::new(authority.account),
    ]
}
