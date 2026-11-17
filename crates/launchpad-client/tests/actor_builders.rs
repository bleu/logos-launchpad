use launchpad_client::build_initialize_namespace_invocation;
use lee_core::account::{AccountId, Actor};

#[test]
fn frozen_namespace_rejects_current_state_purchase_quotes() {
    let namespace = AccountId::new([11; 32]);
    let config = curve_core::Config::decode(
        &borsh::to_vec(&(
            *b"LZNSCONF",
            3u16,
            namespace,
            50u16,
            AccountId::new([13; 32]),
            AccountId::new([14; 32]),
            true,
        ))
        .unwrap(),
    )
    .unwrap();
    let pool = curve_core::PoolAccount {
        buy_policy: curve_core::BuyPolicy::Open,
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
        namespace,
        token0_definition_id: AccountId::new([1; 32]),
        token1_definition_id: AccountId::new([2; 32]),
        owner: AccountId::new([3; 32]),
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .unwrap(),
    };
    assert_eq!(
        launchpad_client::quote_buy(&pool, &config, 200, 0)
            .unwrap_err()
            .to_string(),
        "namespace is frozen"
    );
}

#[test]
fn purchase_preparation_checks_the_participant_cap_with_current_namespace_fees() {
    let namespace = AccountId::new([11; 32]);
    let config = curve_core::Config::decode(
        &borsh::to_vec(&(
            *b"LZNSCONF",
            3u16,
            namespace,
            50u16,
            AccountId::new([13; 32]),
            AccountId::new([14; 32]),
            false,
        ))
        .unwrap(),
    )
    .unwrap();
    let pool = curve_core::PoolAccount {
        buy_policy: curve_core::BuyPolicy::Open,
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
        namespace,
        token0_definition_id: AccountId::new([1; 32]),
        token1_definition_id: AccountId::new([2; 32]),
        owner: AccountId::new([3; 32]),
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .unwrap(),
    };
    let sale = launchpad_client::FactorySale {
        namespace,
        factory_program_id: AccountId::new([4; 32]),
        curve_program_id: AccountId::new([5; 32]),
        launch_salt: [6; 32],
        collateral_definition: pool.token1_definition_id,
    };
    let result = launchpad_client::prepare_buy_invocation(
        sale,
        AccountId::new([7; 32]),
        &pool,
        &config,
        launchpad_client::ExactOutputTradeTerms {
            amount_out: 200,
            max_amount_in: 25,
        },
        None,
        0,
    );
    assert!(matches!(
        result,
        Err(launchpad_client::ClientError::QuoteBuy(
            pool::SwapError::SlippageExceeded
        ))
    ));
}

#[test]
fn timed_purchase_declares_the_clock_actor_used_by_the_curve() {
    let sale = launchpad_client::FactorySale {
        namespace: AccountId::new([1; 32]),
        factory_program_id: AccountId::new([2; 32]),
        curve_program_id: AccountId::new([3; 32]),
        launch_salt: [4; 32],
        collateral_definition: AccountId::new([5; 32]),
    };
    let call = launchpad_client::build_buy_invocation(
        sale,
        AccountId::new([6; 32]),
        launchpad_client::ExactOutputTradeTerms {
            amount_out: 1,
            max_amount_in: 2,
        },
        None,
        0,
    );
    assert!(call.actors.contains(&Actor::new(
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
        clock_core::clock_account_id()
    )));
}

#[test]
fn new_nft_recipient_holding_has_pda_admission_without_signer_authorization() {
    let definition = AccountId::new([1; 32]);
    let owner = AccountId::new([2; 32]);
    let call = launchpad_client::build_create_authority_invocation(
        definition,
        owner,
        "Creator".into(),
        "ipfs://creator".into(),
    );
    let holding = lez_compat_core::token::holding_address(
        launchpad_client::TOKEN_PROGRAM_ACCOUNT_ID,
        owner,
        definition,
    );
    let mentions = call.account_mentions();
    let mention = mentions
        .iter()
        .find(|m| m.actor().account_id == holding)
        .unwrap();
    assert!(!mention.authorizes);
    assert!(
        matches!(&mention.identity, wallet::AccountIdentity::PublicPda { program, seed }
        if *program == launchpad_client::TOKEN_PROGRAM_ACCOUNT_ID && *seed == lez_compat_core::token::holding_seed(owner, definition))
    );
}

#[test]
fn authority_transfer_admits_the_new_recipient_holding_without_its_signature() {
    let definition = AccountId::new([1; 32]);
    let owner = AccountId::new([2; 32]);
    let recipient = AccountId::new([3; 32]);
    let call = launchpad_client::build_transfer_authority_invocation(owner, recipient, definition);
    let holding = lez_compat_core::token::holding_address(
        launchpad_client::TOKEN_PROGRAM_ACCOUNT_ID,
        recipient,
        definition,
    );
    let mentions = call.account_mentions();
    let mention = mentions
        .iter()
        .find(|m| m.actor().account_id == holding)
        .unwrap();
    assert!(!mention.authorizes);
    assert!(matches!(
        &mention.identity,
        wallet::AccountIdentity::PublicPda { .. }
    ));
    assert_eq!(call.signer_accounts, [owner]);
}

#[test]
fn first_purchase_admits_new_participant_output_holding() {
    let namespace = AccountId::new([1; 32]);
    let factory = AccountId::new([2; 32]);
    let participant = AccountId::new([6; 32]);
    let sale = launchpad_client::FactorySale {
        namespace,
        factory_program_id: factory,
        curve_program_id: AccountId::new([3; 32]),
        launch_salt: [4; 32],
        collateral_definition: AccountId::new([5; 32]),
    };
    let definition = factory_core::compute_definition_pda(namespace, factory, [4; 32]);
    let call = launchpad_client::build_buy_invocation(
        sale,
        participant,
        launchpad_client::ExactOutputTradeTerms {
            amount_out: 1,
            max_amount_in: 2,
        },
        None,
        0,
    );
    let holding = lez_compat_core::token::holding_address(
        launchpad_client::TOKEN_PROGRAM_ACCOUNT_ID,
        participant,
        definition,
    );
    let mentions = call.account_mentions();
    let mention = mentions
        .iter()
        .find(|m| m.actor().account_id == holding)
        .unwrap();
    assert!(!mention.authorizes);
    assert!(matches!(
        &mention.identity,
        wallet::AccountIdentity::PublicPda { .. }
    ));
}

#[test]
fn fee_collection_admits_the_current_treasury_holding_without_application_signers() {
    let treasury = AccountId::new([1; 32]);
    let collateral = AccountId::new([2; 32]);
    let call = launchpad_client::build_collect_fees_invocation(
        AccountId::new([3; 32]),
        AccountId::new([4; 32]),
        AccountId::new([5; 32]),
        collateral,
        treasury,
    );
    let holding = lez_compat_core::token::holding_address(
        launchpad_client::TOKEN_PROGRAM_ACCOUNT_ID,
        treasury,
        collateral,
    );
    let mentions = call.account_mentions();
    let mention = mentions
        .iter()
        .find(|m| m.actor().account_id == holding)
        .unwrap();
    assert!(!mention.authorizes);
    assert!(matches!(
        &mention.identity,
        wallet::AccountIdentity::PublicPda { .. }
    ));
    assert!(call.signer_accounts.is_empty());
}

#[test]
fn namespace_initialization_admits_its_config_and_administrator_role_pdas() {
    let namespace = AccountId::new([11; 32]);
    let program = AccountId::new([12; 32]);
    let call =
        build_initialize_namespace_invocation(namespace, program, 0, AccountId::new([13; 32]));
    let mentions = call.account_mentions();
    for account in [
        curve_core::compute_config_pda(namespace, program),
        curve_core::compute_admin_role(namespace, program),
    ] {
        let mention = mentions
            .iter()
            .find(|m| m.actor().account_id == account)
            .unwrap();
        assert!(!mention.authorizes);
        assert!(
            matches!(&mention.identity, wallet::AccountIdentity::PublicPda { program: owner, .. } if *owner == program)
        );
    }
}

#[test]
fn pda_administrator_successor_declares_its_integrating_owner_without_a_key_signature() {
    let owner = AccountId::new([20; 32]);
    let seed = lee_core::program::PdaSeed::new([21; 32]);
    let successor = AccountId::for_public_pda(&owner, &seed);
    let admin = AccountId::new([22; 32]);
    let call = launchpad_client::build_transfer_admin_invocation(
        AccountId::new([23; 32]),
        AccountId::new([24; 32]),
        admin,
        successor,
        curve_core::AdminCandidate::Pda {
            program_id: owner,
            seed: *seed.as_bytes(),
        },
    );
    assert!(call.actors.contains(&Actor::new(successor, owner)));
    assert_eq!(call.signer_accounts, [admin]);
}

#[test]
fn pda_administrator_successor_has_derivation_checked_admission_without_authorization() {
    let owner = AccountId::new([20; 32]);
    let seed = lee_core::program::PdaSeed::new([21; 32]);
    let successor = AccountId::for_public_pda(&owner, &seed);
    let call = launchpad_client::build_transfer_admin_invocation(
        AccountId::new([23; 32]),
        AccountId::new([24; 32]),
        AccountId::new([22; 32]),
        successor,
        curve_core::AdminCandidate::Pda {
            program_id: owner,
            seed: *seed.as_bytes(),
        },
    );
    let mentions = call.account_mentions();
    let mention = mentions
        .iter()
        .find(|mention| mention.actor() == Actor::new(successor, owner))
        .unwrap();
    assert!(!mention.authorizes);
    assert!(
        matches!(mention.identity, wallet::AccountIdentity::PublicPda { program, seed: actual } if program == owner && actual == seed)
    );
}

#[test]
fn creation_request_preserves_unix_seconds_on_the_public_wire() {
    let call = launchpad_client::build_create_sale_invocation(
        launchpad_client::FactorySale {
            namespace: AccountId::new([1; 32]),
            factory_program_id: AccountId::new([2; 32]),
            curve_program_id: AccountId::new([3; 32]),
            launch_salt: [4; 32],
            collateral_definition: AccountId::new([5; 32]),
        },
        AccountId::new([6; 32]),
        launchpad_client::CreateSaleRequest {
            buy_policy: curve_core::BuyPolicy::Open,
            pause_policy: curve_core::PausePolicy::Disabled,
            ending_policy: curve_core::EndingPolicy::Either,
            trade_mode: launchpad_client::TradeMode::BuyAndSell,
            sell_fee_bps: 0,
            graduation_hook: None,
            name: "Sale".into(),
            uri: "ipfs://sale".into(),
            sale_reserve: 800,
            graduation_policy: launchpad_client::GraduationPolicy::default(),
            creator_allocation: 0,
            virtual_token_reserve: 1_000,
            virtual_collateral_reserve: 100,
            end_timestamp: Some(1_700_003_600),
            now: 1_700_000_000,
        },
    )
    .unwrap();
    assert!(matches!(
        &call.instruction,
        factory_core::Message::CreateFactoryPool {
            now: 1_700_000_000,
            end_timestamp: Some(1_700_003_600),
            ..
        }
    ));
    let bytes = call.message_bytes().unwrap();
    assert_eq!(bytes.len(), 280);
    assert_eq!(&bytes[..8], &[0, 0, 0, 3, 1, 0, 0, 0]);
    assert_eq!(&bytes[167..176], &[1, 0x10, 0xff, 0x53, 0x65, 0, 0, 0, 0]);
    assert_eq!(&bytes[272..], &[0x00, 0xf1, 0x53, 0x65, 0, 0, 0, 0]);
}

#[test]
fn namespace_initialization_starts_at_the_signed_namespace_actor() {
    let namespace = AccountId::new([11; 32]);
    let deployment = AccountId::new([12; 32]);
    let treasury = AccountId::new([13; 32]);
    let call = build_initialize_namespace_invocation(namespace, deployment, 75, treasury);

    assert_eq!(call.destination, Actor::new(namespace, deployment));
    assert_eq!(call.signer_accounts, [namespace]);
    assert_eq!(
        call.instruction,
        curve_core::Message::InitializeNamespace {
            namespace,
            protocol_fee_bps: 75,
            treasury,
        }
    );
}
