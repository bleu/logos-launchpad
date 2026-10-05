//! Namespace administration through the library's public account API.

use admin_authority::{AdminCandidate, AdminConfig};
use freeze_authority::{FreezeCandidate, FreezeConfig};
use lee_core::{
    account::{Account, AccountId, AccountWithMetadata},
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

#[expect(
    clippy::too_many_arguments,
    reason = "namespace creation sets both roles"
)]
pub fn initialize_namespace(
    pre: Vec<AccountWithMetadata>,
    namespace: AccountId,
    admin: AccountId,
    protocol_fee_bps: u32,
    treasury: AccountId,
    emergency_authority: AccountId,
    emergency_candidate: crate::EmergencyCandidate,
    program: ProgramId,
) -> Vec<AccountPostState> {
    assert!(
        (2..=4).contains(&pre.len()),
        "InitializeNamespace requires config, namespace proof, administrator and emergency authority"
    );
    for (i, account) in pre.iter().enumerate() {
        assert!(
            !pre[..i]
                .iter()
                .any(|other| other.account_id == account.account_id),
            "Duplicate account input"
        );
    }
    let config = &pre[0];
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
        crate::identity(&pre[1]),
        namespace,
        "Authority is not the namespace identity"
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
    let accepted = pre
        .iter()
        .find(|a| a.account_id == admin)
        .expect("Missing initial administrator");
    let mut embedded = adapt(config);
    embedded.account.data = Vec::from(
        lee_core::account::Data::from(&Config {
            protocol_fee_bps: protocol_fee_bps as u16,
            treasury,
            ..Config::default()
        })
        .as_ref(),
    )
    .try_into()
    .expect("Config fits account data");
    AdminConfig::bootstrap_at(
        &mut embedded,
        Config::ADMIN_OFFSET,
        AdminCandidate::Signer,
        &adapt(accepted),
    )
    .expect("Initial administrator must accept");
    let emergency = pre
        .iter()
        .find(|a| a.account_id == emergency_authority)
        .expect("Missing emergency authority");
    let candidate = match emergency_candidate {
        crate::EmergencyCandidate::Signer => FreezeCandidate::Signer,
        crate::EmergencyCandidate::Pda { program_id, seed } => {
            assert_eq!(
                emergency.account.program_owner, program_id,
                "PDA emergency authority belongs to another program"
            );
            FreezeCandidate::Pda { program_id, seed }
        }
    };
    FreezeConfig::bootstrap_at(
        &mut embedded,
        Config::FREEZE_OFFSET,
        candidate,
        &adapt(emergency),
    )
    .expect("Invalid initial emergency authority");
    let mut post = config.account.clone();
    post.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    let mut posts: Vec<_> = pre
        .iter()
        .map(|a| AccountPostState::new(a.account.clone()))
        .collect();
    posts[0] = AccountPostState::new_claimed_if_default(
        post,
        Claim::Pda(compute_config_pda_seed(namespace)),
    );
    posts
}

pub fn freeze_namespace(
    pre: Vec<AccountWithMetadata>,
    namespace: AccountId,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let [mut config, emergency]: [AccountWithMetadata; 2] = pre
        .try_into()
        .expect("FreezeNamespace requires config and emergency authority");
    crate::validated_config(&config, namespace, program);
    let mut embedded = adapt(&config);
    FreezeConfig::perform_freeze_at(&mut embedded, Config::FREEZE_OFFSET, &adapt(&emergency))
        .expect("Namespace freeze authorization failed");
    config.account.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    vec![
        AccountPostState::new(config.account),
        AccountPostState::new(emergency.account),
    ]
}

pub fn unfreeze_namespace(
    pre: Vec<AccountWithMetadata>,
    namespace: AccountId,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let [mut config, emergency]: [AccountWithMetadata; 2] = pre
        .try_into()
        .expect("UnfreezeNamespace requires config and emergency authority");
    crate::validated_config(&config, namespace, program);
    let mut embedded = adapt(&config);
    FreezeConfig::perform_release_at(&mut embedded, Config::FREEZE_OFFSET, &adapt(&emergency))
        .expect("Namespace unfreeze authorization failed");
    config.account.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    vec![
        AccountPostState::new(config.account),
        AccountPostState::new(emergency.account),
    ]
}

pub fn renounce_admin(
    namespace: AccountId,
    mut config: AccountWithMetadata,
    authority: AccountWithMetadata,
    program: ProgramId,
) -> Vec<AccountPostState> {
    crate::validated_config(&config, namespace, program);
    let mut embedded = adapt(&config);
    AdminConfig::perform_renounce_at(&mut embedded, Config::ADMIN_OFFSET, &adapt(&authority))
        .expect("Namespace renunciation failed");
    config.account.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    vec![
        AccountPostState::new(config.account),
        AccountPostState::new(authority.account),
    ]
}

pub fn update_fee(
    pre: Vec<AccountWithMetadata>,
    namespace: AccountId,
    fee: u32,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let [mut config, admin]: [AccountWithMetadata; 2] = pre
        .try_into()
        .expect("UpdateFee requires config and administrator");
    let mut state = crate::validated_config(&config, namespace, program);
    state.assert_admin(&admin);
    assert!(
        fee < u32::from(MAX_FEE_BPS),
        "Protocol fee must be below 10,000 basis points"
    );
    state.protocol_fee_bps = fee as u16;
    config.account.data = (&state).into();
    vec![
        AccountPostState::new(config.account),
        AccountPostState::new(admin.account),
    ]
}

pub fn update_treasury(
    pre: Vec<AccountWithMetadata>,
    namespace: AccountId,
    treasury: AccountId,
    program: ProgramId,
) -> Vec<AccountPostState> {
    let [mut config, admin]: [AccountWithMetadata; 2] = pre
        .try_into()
        .expect("UpdateTreasury requires config and administrator");
    let mut state = crate::validated_config(&config, namespace, program);
    state.assert_admin(&admin);
    assert_ne!(
        treasury,
        AccountId::default(),
        "Treasury must not be the default key"
    );
    state.treasury = treasury;
    config.account.data = (&state).into();
    vec![
        AccountPostState::new(config.account),
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
    assert!(
        (2..=3).contains(&pre.len()),
        "TransferAdmin requires config, administrator and successor"
    );
    crate::validated_config(&pre[0], namespace, program);
    let successor = pre
        .iter()
        .find(|a| a.account_id == new_admin)
        .expect("Missing successor account");
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
    let mut embedded = adapt(&pre[0]);
    AdminConfig::perform_transfer_at(
        &mut embedded,
        Config::ADMIN_OFFSET,
        &adapt(&pre[1]),
        candidate,
        &adapt(successor),
    )
    .expect("Namespace administration transfer failed");
    let mut config = pre[0].account.clone();
    config.data = embedded
        .account
        .data
        .to_vec()
        .try_into()
        .expect("Config fits account data");
    let mut posts: Vec<_> = pre
        .into_iter()
        .map(|a| AccountPostState::new(a.account))
        .collect();
    posts[0] = AccountPostState::new(config);
    posts
}
