use std::str::FromStr;

use lee::AccountId;
use lee_core::account::Actor;
use lez_compat_core::token;

use crate::{
    AccountLocation, ClientError, PrivateAccountId, PrivateBuyRequest,
    build_create_authority_invocation, build_initialize_namespace_invocation,
    build_private_authority_instruction, build_transfer_authority_invocation,
    build_update_fee_invocation, parse_account_id, validate_private_buy_request,
};

use super::fixtures::{CURVE_PROGRAM_ID, sale};

#[test]
fn account_locations_parse_only_the_location_boundary() {
    let account_id = AccountId::new([11; 32]);
    assert_eq!(
        AccountLocation::from_str(&account_id.to_string()).unwrap(),
        AccountLocation::Public(account_id)
    );
    assert_eq!(
        AccountLocation::from_str(&format!("Private/{account_id}")).unwrap(),
        AccountLocation::Private(account_id)
    );
    assert_eq!(
        parse_account_id(&format!("Public/{account_id}")).unwrap(),
        account_id
    );
    assert!(matches!(
        AccountLocation::from_str(&format!("Unknown/{account_id}")),
        Err(ClientError::UnknownAccountLocationPrefix { .. })
    ));
    assert!(matches!(
        PrivateAccountId::from_str(&format!("Public/{account_id}")),
        Err(ClientError::PrivateAccountIdRequiresPrivateLocation { .. })
    ));
    assert_eq!(
        PrivateAccountId::from_str(&account_id.to_string())
            .unwrap()
            .account_id(),
        account_id
    );
}

#[test]
fn config_initialization_uses_the_curve_config_pda_and_namespace_signature() {
    let namespace = sale().namespace;
    let call = build_initialize_namespace_invocation(
        namespace,
        CURVE_PROGRAM_ID,
        75,
        AccountId::new([4; 32]),
    );
    assert_eq!(call.destination, Actor::new(namespace, CURVE_PROGRAM_ID));
    assert!(call.actors.contains(&Actor::new(
        curve_core::compute_config_pda(namespace, CURVE_PROGRAM_ID),
        CURVE_PROGRAM_ID
    )));
    assert_eq!(call.signer_accounts, [namespace]);
}

#[test]
fn authority_creation_and_transfer_use_all_required_signers() {
    let definition = AccountId::new([71; 32]);
    let holder = AccountId::new([72; 32]);
    let recipient = AccountId::new([74; 32]);
    let creation = build_create_authority_invocation(
        definition,
        holder,
        "Admin".into(),
        "https://example.invalid/admin".into(),
    );
    assert_eq!(creation.signer_accounts, [definition]);
    assert!(
        matches!(creation.instruction, token::Message::Create { kind: token::TokenKind::NftMaster, supply: 1, recipient, .. } if recipient == holder)
    );
    let transfer = build_transfer_authority_invocation(holder, recipient, definition);
    assert_eq!(transfer.signer_accounts, [holder]);
    assert_eq!(
        transfer.instruction,
        token::Message::Transfer {
            definition,
            recipient,
            amount: 1
        }
    );
}

#[test]
fn private_authority_builder_reports_unsupported_public_holding_route() {
    let holder = AccountId::new([61; 32]);
    let action = build_update_fee_invocation(sale().namespace, CURVE_PROGRAM_ID, holder, 50);
    assert!(matches!(
        build_private_authority_instruction(&action, holder),
        Err(ClientError::UnsupportedPrivateAuthority)
    ));
}

fn private_buy_request() -> PrivateBuyRequest {
    PrivateBuyRequest {
        amount_out: 25,
        max_collateral_in: 100,
        from_private: PrivateAccountId::from_str(&AccountId::new([6; 32]).to_string()).unwrap(),
        to_private: PrivateAccountId::from_str(&AccountId::new([7; 32]).to_string()).unwrap(),
        gas_reserve: 1,
    }
}

#[test]
fn private_buy_route_has_explicit_unsupported_label() {
    let error = ClientError::UnsupportedPrivateRoute;
    let label: &'static str = (&error).into();
    assert_eq!(label, "unsupported_private_route");
}

#[test]
fn private_buy_requires_private_account_ids_and_atomic_funding() {
    validate_private_buy_request(private_buy_request()).unwrap();
}

#[test]
fn private_buy_rejects_a_zero_gas_reserve() {
    let mut request = private_buy_request();
    request.gas_reserve = 0;
    assert!(matches!(
        validate_private_buy_request(request),
        Err(ClientError::ZeroPrivateBuyGasReserve)
    ));
}

#[test]
fn private_buy_rejects_zero_trade_amounts() {
    let mut output_request = private_buy_request();
    output_request.amount_out = 0;
    assert!(matches!(
        validate_private_buy_request(output_request),
        Err(ClientError::ZeroPrivateBuyAmount)
    ));

    let mut collateral_request = private_buy_request();
    collateral_request.max_collateral_in = 0;
    assert!(matches!(
        validate_private_buy_request(collateral_request),
        Err(ClientError::ZeroPrivateBuyAmount)
    ));
}
