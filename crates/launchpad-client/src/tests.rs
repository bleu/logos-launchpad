use std::str::FromStr;

use factory_core::{CreationStage, FactoryState, SettlementStage};
use lee::{AccountId, program::Program};

use super::{
    AccountLocation, ClientError, CreateSaleRequest, ExactInputTradeTerms, ExactOutputTradeTerms,
    FactorySale, PoolFeeTarget, PrivateAccountId, PrivateBuyRequest, build_buy_invocation,
    build_buy_with_collateral_invocation, build_claim_creator_allocation_invocation,
    build_collect_fees_invocation, build_create_authority_invocation, build_create_sale_invocation,
    build_private_authority_instruction, build_sell_invocation,
    build_transfer_authority_invocation, build_update_config_invocation,
    build_withdraw_factory_proceeds_invocation, parse_account_id, quote_buy,
    quote_buy_with_collateral, quote_sell, validate_factory_state, validate_private_buy_request,
};

const FACTORY_PROGRAM_ID: [u32; 8] = [7; 8];
const CURVE_PROGRAM_ID: [u32; 8] = [6; 8];

fn sale() -> FactorySale {
    FactorySale {
        namespace: AccountId::new([0xAD; 32]),
        factory_program_id: FACTORY_PROGRAM_ID,
        curve_program_id: CURVE_PROGRAM_ID,
        launch_salt: [1; 32],
        collateral_definition: AccountId::new([5; 32]),
    }
}

fn creation() -> CreateSaleRequest {
    CreateSaleRequest {
        trade_mode: factory_core::TradeMode::BuyAndSell,
        graduation_hook: None,
        name: "E2E token".into(),
        uri: "https://example.invalid/e2e-token.json".into(),
        sale_reserve: 800,
        dex_seed_reserve: 100,
        creator_allocation: 50,
        virtual_token_reserve: 2_000,
        virtual_collateral_reserve: 100,
        end_timestamp: None,
    }
}

fn pool_snapshot() -> (curve_core::PoolAccount, curve_core::Config) {
    let pool = curve_core::PoolAccount {
        allowed_input_token: None,
        funded: true,
        namespace: AccountId::new([0xAD; 32]),
        token0_definition_id: AccountId::new([2; 32]),
        token1_definition_id: AccountId::new([5; 32]),
        owner: AccountId::new([1; 32]),
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 100,
            virtual_reserve0: 1_000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .unwrap(),
    };
    let config = curve_core::Config {
        admin: AccountId::new([3; 32]),
        protocol_fee_bps: 0,
        treasury: AccountId::new([4; 32]),
        ..curve_core::Config::default()
    };
    (pool, config)
}

fn valid_factory_state(sale: FactorySale) -> FactoryState {
    let addresses = super::factory_pool_addresses(sale);
    FactoryState {
        trade_mode: factory_core::TradeMode::BuyAndSell,
        graduation_hook: None,
        creation_stage: CreationStage::Active,
        settlement_stage: SettlementStage::Ready,
        end_timestamp: None,
        namespace: sale.namespace,
        launch_salt: sale.launch_salt,
        token_definition_id: addresses.token_definition,
        collateral_definition_id: sale.collateral_definition,
        sale_reserve: 800,
        dex_seed_reserve: 100,
        creator_allocation: 50,
        total_supply: 950,
        virtual_token_reserve: 2_000,
        virtual_collateral_reserve: 100,
        curve_program_id: sale.curve_program_id,
        creator_commitment: [0; 32],
        creator_escrow_id: addresses.creator_escrow,
        pool_id: addresses.pool,
        creator_allocation_claimed: false,
    }
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
fn creation_request_encodes_the_required_typed_mode() {
    for (mode, wire) in [
        (factory_core::TradeMode::BuyOnly, 0),
        (factory_core::TradeMode::BuyAndSell, 1),
    ] {
        let mut request = creation();
        request.trade_mode = mode;
        let call = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).unwrap();
        assert!(
            matches!(call.instruction, factory_core::Instruction::CreateFactoryPool { trade_mode, .. } if trade_mode == wire)
        );
    }
}

#[test]
fn resumed_creation_cannot_change_trade_mode_at_any_stage() {
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
        CreationStage::Active,
    ] {
        let mut state = valid_factory_state(sale());
        state.creation_stage = stage;
        let mut request = creation();
        assert!(
            super::next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        request.trade_mode = factory_core::TradeMode::BuyOnly;
        assert!(matches!(
            super::next_creation_invocation(sale(), AccountId::new([9; 32]), request, Some(&state)),
            Err(ClientError::LaunchParametersMismatch)
        ));
    }
}

#[test]
fn factory_launch_builds_derived_accounts_and_creator_authorization() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let invocation = build_create_sale_invocation(sale, creator, creation()).unwrap();
    let addresses = super::factory_pool_addresses(sale);

    assert_eq!(invocation.program_id, FACTORY_PROGRAM_ID);
    assert_eq!(invocation.signer_accounts, vec![creator]);
    assert_eq!(
        invocation.account_ids[..6],
        [
            addresses.factory,
            addresses.token_definition,
            factory_core::compute_mint_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt
            ),
            factory_core::compute_metadata_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt
            ),
            addresses.creator_escrow,
            creator,
        ]
    );
    assert_eq!(invocation.account_ids.len(), 15);
    assert_eq!(invocation.account_ids[7], sale.collateral_definition);
}

#[test]
fn factory_launch_rejects_a_virtual_token_reserve_at_the_sale_target() {
    let mut request = creation();
    request.virtual_token_reserve = request.sale_reserve;
    let error = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).unwrap_err();

    assert!(matches!(
        error,
        ClientError::FactoryParameters(
            factory_core::CreateError::VirtualTokenReserveNotAboveSaleReserve
        )
    ));
}

#[test]
fn namespace_selection_changes_factory_pool_and_swap_config_together() {
    let mut other_sale = sale();
    other_sale.namespace = AccountId::new([0xAE; 32]);
    let participant = AccountId::new([3; 32]);
    let terms = ExactOutputTradeTerms {
        amount_out: 10,
        max_amount_in: 100,
    };
    let first = build_buy_invocation(sale(), participant, terms);
    let second = build_buy_invocation(other_sale, participant, terms);

    assert_ne!(first.account_ids[0], second.account_ids[0]);
    assert_eq!(
        first.account_ids[1],
        curve_core::compute_config_pda(sale().namespace, CURVE_PROGRAM_ID)
    );
    assert_eq!(
        second.account_ids[1],
        curve_core::compute_config_pda(other_sale.namespace, CURVE_PROGRAM_ID)
    );
    assert_ne!(first.account_ids[4], second.account_ids[4]);
    assert_ne!(first.account_ids[5], second.account_ids[5]);
}

#[test]
fn creation_and_trade_builders_use_one_factory_sale_context() {
    let sale = sale();
    let participant = AccountId::new([9; 32]);
    let creation = build_create_sale_invocation(sale, participant, creation()).unwrap();
    let exact_output = build_buy_invocation(
        sale,
        participant,
        ExactOutputTradeTerms {
            amount_out: 25,
            max_amount_in: 100,
        },
    );
    let exact_input = build_buy_with_collateral_invocation(
        sale,
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
    );
    let sale_input = build_sell_invocation(
        sale,
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
    );

    assert_eq!(creation.program_id, FACTORY_PROGRAM_ID);
    assert_eq!(exact_output.account_ids[0], exact_input.account_ids[0]);
    assert_eq!(exact_input.account_ids[0], sale_input.account_ids[0]);
    assert_eq!(exact_output.signer_accounts, vec![participant]);
    assert_eq!(exact_output.account_ids[2], participant);
}

#[test]
fn config_initialization_uses_the_curve_config_pda_and_admin_signature() {
    let admin = AccountId::new([9; 32]);
    let treasury = AccountId::new([4; 32]);
    let invocation = build_update_config_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        admin,
        admin,
        75,
        treasury,
    );

    assert_eq!(invocation.program_id, CURVE_PROGRAM_ID);
    assert_eq!(
        invocation.account_ids,
        vec![
            curve_core::compute_config_pda(sale().namespace, CURVE_PROGRAM_ID),
            admin,
        ]
    );
    assert_eq!(invocation.signer_accounts, vec![admin]);
    assert!(matches!(
        invocation.instruction,
        curve_core::Instruction::UpdateConfig {
            admin: actual_admin,
            protocol_fee_bps: 75,
            treasury: actual_treasury,
            ..
        } if actual_admin == admin && actual_treasury == treasury
    ));
}

#[test]
fn authority_creation_and_transfer_use_all_required_signers() {
    let definition = AccountId::new([71; 32]);
    let holder = AccountId::new([72; 32]);
    let metadata = AccountId::new([73; 32]);
    let creation = build_create_authority_invocation(
        definition,
        holder,
        metadata,
        "Admin".into(),
        "https://example.invalid/admin".into(),
    );
    assert_eq!(creation.signer_accounts, vec![definition, holder, metadata]);
    assert!(matches!(
        creation.instruction,
        token_core::Instruction::NewDefinitionWithMetadata {
            new_definition: token_core::NewTokenDefinition::NonFungible {
                printable_supply: 1,
                ..
            },
            ..
        }
    ));

    let transfer = build_transfer_authority_invocation(holder, AccountId::new([74; 32]));
    assert_eq!(
        transfer.signer_accounts,
        vec![holder, AccountId::new([74; 32])]
    );
    assert!(matches!(
        transfer.instruction,
        token_core::Instruction::Transfer {
            amount_to_transfer: 1
        }
    ));
}

#[test]
fn private_authority_builder_preserves_action_and_locates_holder() {
    let holder = AccountId::new([61; 32]);
    let action = build_update_config_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        holder,
        sale().namespace,
        50,
        AccountId::new([63; 32]),
    );
    let routed = build_private_authority_instruction(&action, holder).unwrap();

    assert_eq!(routed.authority_index, 1);
    let decoded: curve_core::Instruction =
        risc0_zkvm::serde::from_slice(&routed.instruction_data).unwrap();
    assert_eq!(
        decoded,
        curve_core::Instruction::Private {
            instruction: Box::new(action.instruction.clone())
        }
    );
    assert!(matches!(
        build_private_authority_instruction(&action, AccountId::new([64; 32])),
        Err(ClientError::InvalidAuthoritySigners)
    ));
}

#[test]
fn factory_instruction_wire_slots_follow_the_source_idl_order() {
    let encodings: [(factory_core::Instruction, &[u32]); 4] = [
        (
            factory_core::Instruction::CreateFactoryPool {
                trade_mode: 1,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: sale().namespace,
                launch_salt: sale().launch_salt,
                name: "Sale".into(),
                uri: "ipfs://sale".into(),
                sale_reserve: 800,
                dex_seed_reserve: 100,
                creator_allocation: 50,
                virtual_token_reserve: 2_000,
                virtual_collateral_reserve: 100,
                end_timestamp: None,
                curve_program_id: CURVE_PROGRAM_ID,
            },
            &[0],
        ),
        (factory_core::Instruction::ContinueCreation, &[1]),
        (factory_core::Instruction::ClaimCreatorAllocation, &[2]),
        (factory_core::Instruction::WithdrawFactoryProceeds, &[3]),
    ];
    for (instruction, expected) in encodings {
        assert_eq!(
            &Program::serialize_instruction(instruction).unwrap()[..1],
            expected
        );
    }
}

#[test]
fn creator_claim_uses_factory_escrow_and_derived_recipient_ata() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let addresses = super::factory_pool_addresses(sale);
    let invocation = build_claim_creator_allocation_invocation(sale, creator);

    assert_eq!(invocation.program_id, FACTORY_PROGRAM_ID);
    assert_eq!(invocation.signer_accounts, vec![creator]);
    assert_eq!(invocation.account_ids[0], addresses.factory);
    assert_eq!(invocation.account_ids[2], addresses.creator_escrow);
    assert_eq!(invocation.account_ids[4], addresses.token_definition);
    assert_eq!(
        invocation.account_ids[5],
        super::associated_token_account(creator, addresses.token_definition)
    );
    assert!(matches!(
        invocation.instruction,
        factory_core::Instruction::ClaimCreatorAllocation
    ));
}

#[test]
fn saved_settlement_stages_identify_completed_manual_work() {
    for (stage, complete) in [
        (SettlementStage::Complete, true),
        (SettlementStage::NoPayout, true),
        (SettlementStage::Graduated, true),
        (SettlementStage::Unstarted, false),
        (SettlementStage::Ready, false),
        (
            SettlementStage::Withdrawn {
                unsold: 0,
                collateral_due: 0,
            },
            false,
        ),
        (SettlementStage::Burned { collateral_due: 0 }, false),
        (SettlementStage::TokenPaid { collateral_due: 0 }, false),
        (SettlementStage::HookReady { collateral_due: 0 }, false),
    ] {
        let mut state = valid_factory_state(sale());
        state.settlement_stage = stage;
        assert_eq!(
            state.settlement_stage.is_manual_settlement_complete(),
            complete,
            "{stage:?}"
        );
    }
}

#[test]
fn settlement_builders_keep_factory_account_order() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let claim = build_claim_creator_allocation_invocation(sale, creator);
    let withdrawal =
        build_withdraw_factory_proceeds_invocation(sale, creator, AccountId::new([4; 32]));

    assert_eq!(claim.account_ids.len(), 7);
    assert_eq!(withdrawal.account_ids.len(), 14);
    assert_eq!(claim.signer_accounts, vec![creator]);
    assert_eq!(withdrawal.signer_accounts, vec![creator]);
}

#[test]
fn exact_output_buy_uses_collateral_input_and_expected_account_order() {
    let sale = sale();
    let participant = AccountId::new([9; 32]);
    let addresses = super::factory_pool_addresses(sale);
    let invocation = build_buy_invocation(
        sale,
        participant,
        ExactOutputTradeTerms {
            amount_out: 25,
            max_amount_in: 100,
        },
    );

    assert_eq!(invocation.program_id, CURVE_PROGRAM_ID);
    assert_eq!(invocation.signer_accounts, vec![participant]);
    assert_eq!(invocation.account_ids[0], addresses.pool);
    assert_eq!(invocation.account_ids[2], participant);
    assert_eq!(
        invocation.account_ids[3],
        super::associated_token_account(participant, sale.collateral_definition)
    );
    assert_eq!(
        invocation.account_ids[6],
        super::associated_token_account(participant, addresses.token_definition)
    );
    assert_eq!(
        invocation.account_ids[7],
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID
    );
    assert!(matches!(
        invocation.instruction,
        curve_core::Instruction::SwapExactOutput {
            amount_out: 25,
            max_amount_in: 100,
            token_in,
        } if token_in == sale.collateral_definition
    ));
}

#[test]
fn collateral_buy_uses_collateral_input_and_expected_account_order() {
    let sale = sale();
    let participant = AccountId::new([9; 32]);
    let addresses = super::factory_pool_addresses(sale);
    let invocation = build_buy_with_collateral_invocation(
        sale,
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 100,
        },
    );

    assert_eq!(invocation.program_id, CURVE_PROGRAM_ID);
    assert_eq!(invocation.signer_accounts, vec![participant]);
    assert_eq!(
        invocation.account_ids[3],
        super::associated_token_account(participant, sale.collateral_definition)
    );
    assert_eq!(
        invocation.account_ids[6],
        super::associated_token_account(participant, addresses.token_definition)
    );
    assert_eq!(
        invocation.account_ids[7],
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID
    );
    assert!(matches!(
        invocation.instruction,
        curve_core::Instruction::SwapExactInput {
            amount_in: 25,
            min_amount_out: 100,
            token_in,
        } if token_in == sale.collateral_definition
    ));
}

#[test]
fn sell_uses_launch_token_input_and_expected_account_order() {
    let sale = sale();
    let participant = AccountId::new([9; 32]);
    let addresses = super::factory_pool_addresses(sale);
    let invocation = build_sell_invocation(
        sale,
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
    );

    assert_eq!(invocation.program_id, CURVE_PROGRAM_ID);
    assert_eq!(invocation.signer_accounts, vec![participant]);
    assert_eq!(
        invocation.account_ids[3],
        super::associated_token_account(participant, addresses.token_definition)
    );
    assert_eq!(
        invocation.account_ids[6],
        super::associated_token_account(participant, sale.collateral_definition)
    );
    assert_eq!(
        invocation.account_ids[7],
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID
    );
    assert!(matches!(
        invocation.instruction,
        curve_core::Instruction::SwapExactInput {
            amount_in: 25,
            min_amount_out: 10,
            token_in,
        } if token_in == addresses.token_definition
    ));
}

#[test]
fn buy_only_quotes_reject_selling_and_keep_both_buy_forms() {
    let (mut pool, config) = pool_snapshot();
    pool.allowed_input_token = Some(pool::TokenSide::Token1);
    assert!(matches!(
        quote_sell(&pool, &config, 250, 1),
        Err(ClientError::InputTokenRestricted)
    ));
    assert_eq!(quote_buy(&pool, &config, 200, 1).unwrap().amount_in, 25);
    assert_eq!(
        quote_buy_with_collateral(&pool, &config, 25, 1)
            .unwrap()
            .amount_out,
        200
    );
}

#[test]
fn token0_only_pool_quotes_reject_both_collateral_buy_forms() {
    let (mut pool, config) = pool_snapshot();
    pool.allowed_input_token = Some(pool::TokenSide::Token0);
    let quotes = [
        quote_buy(&pool, &config, 200, 1),
        quote_buy_with_collateral(&pool, &config, 25, 1),
    ];
    for (form, quote) in ["exact-output", "exact-input"].into_iter().zip(quotes) {
        assert!(
            matches!(quote, Err(ClientError::InputTokenRestricted)),
            "{form} buy returned {quote:?}"
        );
    }
}

#[test]
fn quote_builders_match_pool_snapshot_rules_and_map_errors() {
    let (pool, config) = pool_snapshot();
    let exact_output = quote_buy(&pool, &config, 200, 1).unwrap();
    let exact_input = quote_buy_with_collateral(&pool, &config, 25, 1).unwrap();
    let sell = quote_sell(&pool, &config, 250, 1).unwrap();

    assert_eq!((exact_output.amount_in, exact_output.amount_out), (25, 200));
    assert_eq!((exact_input.amount_in, exact_input.amount_out), (25, 200));
    assert_eq!((sell.amount_in, sell.amount_out), (250, 20));
    assert_eq!(pool.pool.k, 100_000);
    assert_eq!(pool.pool.real_reserve0, 800);
    assert_eq!(pool.pool.real_reserve1, 100);
    assert!(matches!(
        quote_buy(&pool, &config, 801, 1),
        Err(ClientError::QuoteBuy(
            pool::SwapError::InsufficientRealOutputReserve
        ))
    ));
    assert!(matches!(
        quote_sell(&pool, &config, 0, 1),
        Err(ClientError::QuoteSell(pool::SwapError::ZeroAmount))
    ));
    assert!(matches!(
        quote_buy_with_collateral(&pool, &config, 0, 1),
        Err(ClientError::QuoteBuyWithCollateral(
            pool::SwapError::ZeroAmount
        ))
    ));
    let mut pending = pool;
    pending.funded = false;
    assert!(matches!(
        quote_buy(&pending, &config, 1, 1),
        Err(ClientError::PoolFundingPending)
    ));
}

#[test]
fn collection_is_unsigned_and_factory_withdrawal_removes_duplicate_treasury_ata() {
    let sale = sale();
    let creator = AccountId::new([2; 32]);
    let pool = AccountId::new([4; 32]);
    let collection = build_collect_fees_invocation(
        sale.namespace,
        sale.curve_program_id,
        pool,
        sale.collateral_definition,
        creator,
    );
    assert!(collection.signer_accounts.is_empty());
    assert_eq!(
        collection.account_ids,
        vec![
            pool,
            curve_core::compute_config_pda(sale.namespace, sale.curve_program_id),
            super::associated_token_account(pool, sale.collateral_definition),
            super::associated_token_account(creator, sale.collateral_definition),
        ]
    );
    assert_eq!(collection.instruction, curve_core::Instruction::CollectFees);

    let withdrawal = build_withdraw_factory_proceeds_invocation(sale, creator, creator);
    assert_eq!(withdrawal.account_ids.len(), 13);
    assert_eq!(
        withdrawal
            .account_ids
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        13
    );
}

#[test]
fn collection_builder_omits_the_treasury_when_it_is_the_pool_vault() {
    let sale = sale();
    let pool = AccountId::new([4; 32]);
    let call = build_collect_fees_invocation(
        sale.namespace,
        sale.curve_program_id,
        pool,
        sale.collateral_definition,
        pool,
    );
    assert_eq!(
        call.account_ids,
        vec![
            pool,
            curve_core::compute_config_pda(sale.namespace, sale.curve_program_id),
            super::associated_token_account(pool, sale.collateral_definition),
        ]
    );
    assert!(call.signer_accounts.is_empty());
    assert_eq!(call.instruction, curve_core::Instruction::CollectFees);
}

#[test]
fn graduation_builder_is_unsigned_and_preserves_remaining_account_order() {
    let mut state = valid_factory_state(sale());
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: [9; 8],
        configuration: vec![7, 11],
    });
    let remaining = vec![AccountId::new([51; 32]), AccountId::new([52; 32])];
    let call = super::build_graduation_invocation(
        sale(),
        &state,
        AccountId::new([4; 32]),
        remaining.clone(),
    )
    .unwrap();
    assert!(call.signer_accounts.is_empty());
    assert_eq!(call.program_id, FACTORY_PROGRAM_ID);
    assert_eq!(call.account_ids.len(), 16);
    assert_eq!(&call.account_ids[14..], &remaining);
    assert_eq!(
        Program::serialize_instruction(call.instruction).unwrap(),
        vec![4, 2]
    );
}

#[test]
fn factory_withdrawal_routes_reserves_through_factory_policy() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let addresses = super::factory_pool_addresses(sale);
    let invocation =
        build_withdraw_factory_proceeds_invocation(sale, creator, AccountId::new([4; 32]));

    assert_eq!(invocation.program_id, FACTORY_PROGRAM_ID);
    assert_eq!(invocation.signer_accounts, vec![creator]);
    assert_eq!(invocation.account_ids.len(), 14);
    assert_eq!(invocation.account_ids[0], addresses.factory);
    assert_eq!(invocation.account_ids[3], addresses.token_definition);
    assert_eq!(
        invocation.account_ids[9],
        super::associated_token_account(creator, addresses.token_definition)
    );
    assert_eq!(
        invocation.account_ids[11],
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID
    );
    assert!(matches!(
        invocation.instruction,
        factory_core::Instruction::WithdrawFactoryProceeds
    ));
}

#[test]
fn private_buy_uses_the_execute_envelope_and_idl_field_name() {
    let sale = sale();
    let instruction = private_flow_core::PrivateBuyInstruction::Execute {
        curve_program_id: sale.curve_program_id,
        token_program_id: [11; 8],
        ata_program_id: [12; 8],
        native_transfer_program_id: [13; 8],
        amount_out: 25,
        max_collateral_in: 100,
        gas_reserve: 1,
        collateral_definition_id: sale.collateral_definition,
    };

    let encoded = Program::serialize_instruction(instruction.clone()).unwrap();
    assert_eq!(encoded[0], 0);
    let decoded: private_flow_core::PrivateBuyInstruction =
        risc0_zkvm::serde::from_slice(&encoded).unwrap();
    assert_eq!(decoded, instruction);
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

#[test]
fn fee_targets_are_named() {
    let target = PoolFeeTarget {
        pool: AccountId::new([1; 32]),
        collateral_definition: AccountId::new([2; 32]),
    };
    assert_ne!(target.pool, target.collateral_definition);
}

#[test]
fn factory_state_accepts_matching_derived_ids() {
    let sale = sale();
    assert!(validate_factory_state(sale, &valid_factory_state(sale)).is_ok());
}

#[test]
fn factory_state_rejects_a_mismatched_derived_token_definition_id() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.token_definition_id = AccountId::new([91; 32]);
    assert!(matches!(
        validate_factory_state(sale, &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn factory_state_rejects_a_mismatched_derived_creator_escrow_id() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.creator_escrow_id = AccountId::new([92; 32]);
    assert!(matches!(
        validate_factory_state(sale, &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn factory_state_rejects_a_mismatched_derived_pool_id() {
    let sale = sale();
    let mut state = valid_factory_state(sale);
    state.pool_id = AccountId::new([93; 32]);
    assert!(matches!(
        validate_factory_state(sale, &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}
