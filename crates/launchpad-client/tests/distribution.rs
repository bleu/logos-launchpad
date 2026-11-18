use launchpad_client::{TOKEN_PROGRAM_ACCOUNT_ID, distribution::*};
use lee_core::account::AccountId;
fn id(n: u8) -> AccountId {
    AccountId::new([n; 32])
}
#[test]
fn destination_admission_covers_holdings_and_never_grants_signatures() {
    let call = initialize(id(9), id(7), id(40), [3; 32], GraduationIntentKind::Curve);
    let custody = call.destination.account_id;
    let config = Configuration {
        version: VERSION,
        launch_rules: vec![
            Rule {
                destination: id(80),
                bps: 5000,
            },
            Rule {
                destination: id(81),
                bps: 5000,
            },
        ],
        collateral_rules: vec![Rule {
            destination: id(80),
            bps: 10000,
        }],
    };
    let call = admit_destinations(call, &config, custody, [id(30), id(31)]).unwrap();
    assert!(call.signer_accounts.is_empty());
    for (owner, asset) in [
        (id(80), id(30)),
        (id(81), id(30)),
        (id(80), id(31)),
        (custody, id(30)),
        (custody, id(31)),
    ] {
        let address =
            lez_compat_core::token::holding_address(TOKEN_PROGRAM_ACCOUNT_ID, owner, asset);
        assert!(call.actors.iter().any(|actor| actor.account_id == address
            && actor.program_account_id == TOKEN_PROGRAM_ACCOUNT_ID));
        assert!(call.pda_accounts.contains(&(
            TOKEN_PROGRAM_ACCOUNT_ID,
            lez_compat_core::token::holding_seed(owner, asset)
        )));
    }
    assert_eq!(
        Configuration::decode(&encode_configuration(&config).unwrap()).unwrap(),
        config
    );
    let plan = config.plan([101, 11]).unwrap();
    assert_eq!(
        plan.iter().map(|p| p.amount).collect::<Vec<_>>(),
        [50, 51, 11]
    );
}
#[test]
fn initialization_wire_matches_the_factory_contract() {
    let call = initialize(
        id(9),
        id(7),
        id(40),
        [3; 32],
        GraduationIntentKind::Weighted,
    );
    assert_eq!(
        call.message_bytes().unwrap(),
        borsh::to_vec(&factory_core::AdapterMessage::Initialize {
            factory_program_id: id(7),
            namespace: id(40),
            launch_salt: [3; 32],
            accepted_intent: GraduationIntentKind::Weighted
        })
        .unwrap()
    );
    assert_eq!(
        call.pda_accounts,
        vec![(
            id(9),
            factory_core::compute_graduation_seed(factory_core::compute_factory_pda(
                id(40),
                id(7),
                [3; 32]
            ))
        )]
    );
}
