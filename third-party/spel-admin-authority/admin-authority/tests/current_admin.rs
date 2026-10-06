use admin_authority::{AdminCandidate, AdminConfig};
use spel_framework::prelude::*;

#[test]
fn current_admin_tracks_the_public_lifecycle() {
    let current = AccountWithMetadata {
        account: Account::default(),
        account_id: AccountId::new([1; 32]),
        is_authorized: true,
    };
    let successor = AccountWithMetadata {
        account_id: AccountId::new([2; 32]),
        ..current.clone()
    };
    let mut account = AccountWithMetadata {
        account: Account {
            data: vec![0; 48].try_into().unwrap(),
            ..Account::default()
        },
        ..current.clone()
    };
    AdminConfig::bootstrap_at(&mut account, 8, AdminCandidate::Signer, &current).unwrap();
    assert_eq!(
        AdminConfig::from_account_at(&account, 8)
            .unwrap()
            .current_admin(),
        Some(current.account_id)
    );
    AdminConfig::perform_transfer_at(
        &mut account,
        8,
        &current,
        AdminCandidate::Signer,
        &successor,
    )
    .unwrap();
    assert_eq!(
        AdminConfig::from_account_at(&account, 8)
            .unwrap()
            .current_admin(),
        Some(successor.account_id)
    );
    AdminConfig::perform_renounce_at(&mut account, 8, &successor).unwrap();
    assert_eq!(
        AdminConfig::from_account_at(&account, 8)
            .unwrap()
            .current_admin(),
        None
    );
}
