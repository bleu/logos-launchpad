use std::str::FromStr;

use super::GraduationPolicy;
use factory_core::{CreationStage, FactoryState, SettlementStage};
use lee::AccountId;
use lee_core::account::Actor;

use super::{
    AccountLocation, ClientError, CreateSaleRequest, ExactInputTradeTerms, ExactOutputTradeTerms,
    FactorySale, PoolFeeTarget, PrivateAccountId, PrivateBuyRequest, build_buy_invocation,
    build_buy_with_collateral_invocation, build_claim_creator_allocation_invocation,
    build_collect_fees_invocation, build_create_authority_invocation, build_create_sale_invocation,
    build_initialize_namespace_invocation, build_private_authority_instruction,
    build_sell_invocation, build_transfer_authority_invocation, build_update_fee_invocation,
    build_withdraw_factory_proceeds_invocation, parse_account_id, quote_buy,
    quote_buy_with_collateral, quote_sell, validate_factory_state, validate_private_buy_request,
};

const FACTORY_PROGRAM_ID: AccountId = AccountId::new([7; 32]);
const CURVE_PROGRAM_ID: AccountId = AccountId::new([6; 32]);

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
        buy_policy: curve_core::BuyPolicy::Open,
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Depletion,
        trade_mode: factory_core::TradeMode::BuyAndSell,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: GraduationPolicy::default(),
        name: "E2E token".into(),
        uri: "https://example.invalid/e2e-token.json".into(),
        graduation_reserve: 0,
        sale_reserve: 800,
        creator_allocation: 50,
        virtual_token_reserve: 2_000,
        virtual_collateral_reserve: 100,
        end_timestamp: None,
        now: 0,
    }
}

fn pool_snapshot() -> (curve_core::PoolAccount, curve_core::Config) {
    let pool = curve_core::PoolAccount {
        buy_policy: curve_core::BuyPolicy::Open,
        trade_control: curve_core::TradeControl::Unrestricted,
        allowed_input_token: None,
        funded: true,
        closure_observed: false,
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
            ending_policy: curve_core::EndingPolicy::Manual,
            sell_fee_bps: 0,
            close_timestamp: None,
            close_on_depletion: None,
        })
        .unwrap(),
    };
    let config = curve_core::Config::decode(
        &borsh::to_vec(&(
            *b"LZNSCONF",
            3u16,
            pool.namespace,
            0u16,
            AccountId::new([4; 32]),
            AccountId::new([3; 32]),
            false,
        ))
        .unwrap(),
    )
    .unwrap();
    (pool, config)
}

fn valid_factory_state(sale: FactorySale) -> FactoryState {
    let addresses = super::factory_pool_addresses(sale);
    let mut bytes = Vec::new();
    macro_rules! field {
        ($value:expr) => {
            borsh::BorshSerialize::serialize(&$value, &mut bytes).unwrap()
        };
    }
    field!(curve_core::BuyPolicy::Open);
    field!(curve_core::PausePolicy::Disabled);
    field!(curve_core::EndingPolicy::Depletion);
    field!(factory_core::TradeMode::BuyAndSell);
    field!(0u16);
    field!(None::<factory_core::GraduationHook>);
    field!(CreationStage::Active);
    field!(SettlementStage::Ready);
    field!(None::<u64>);
    field!(sale.namespace);
    field!(sale.launch_salt);
    field!(addresses.token_definition);
    field!(sale.collateral_definition);
    field!(800u128);
    field!(0u128); // Graduation reserve.
    field!(GraduationPolicy::default());
    field!(50u128);
    field!(850u128);
    field!(2_000u128);
    field!(100u128);
    field!(sale.curve_program_id);
    field!(factory_core::compute_curve_creator_role_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt
    ));
    field!("E2E token".to_owned());
    field!("https://example.invalid/e2e-token.json".to_owned());
    field!(addresses.creator_escrow);
    field!(addresses.pool);
    field!(false);
    field!(None::<u8>);
    FactoryState::decode(&bytes).unwrap()
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

fn lbp_sale() -> super::LbpSale {
    super::LbpSale {
        namespace: sale().namespace,
        namespace_program_id: sale().curve_program_id,
        factory_program_id: sale().factory_program_id,
        lbp_program_id: AccountId::new([12; 32]),
        launch_salt: sale().launch_salt,
        collateral_definition: sale().collateral_definition,
    }
}

#[test]
fn fixed_price_exact_input_builder_keeps_membership_and_trader_authority() {
    let selected = lbp_sale();
    let trader = AccountId::new([9; 32]);
    let membership = curve_core::MembershipPath {
        index: 3,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let call = super::build_fixed_price_buy_with_collateral_invocation(
        selected,
        trader,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        Some(membership.clone()),
        9,
    );
    let factory = factory_core::compute_factory_pda(
        selected.namespace,
        selected.factory_program_id,
        selected.launch_salt,
    );
    let definition = factory_core::compute_definition_pda(
        selected.namespace,
        selected.factory_program_id,
        selected.launch_salt,
    );
    let pool = lbp_core::compute_pool_pda(
        selected.namespace,
        selected.lbp_program_id,
        definition,
        selected.collateral_definition,
        factory,
    );
    assert_eq!(
        call.destination,
        Actor::new(trader, selected.lbp_program_id)
    );
    assert_eq!(call.signer_accounts, [trader]);
    assert_eq!(
        call.instruction,
        lbp_core::Message::Execute {
            pool,
            instruction: Box::new(lbp_core::Message::BuyExactInput {
                amount_in: 25,
                min_amount_out: 10,
                membership: Some(membership),
                now: 9,
            })
        }
    );
    assert!(call.actors.contains(&Actor::new(
        curve_core::compute_config_pda(selected.namespace, selected.namespace_program_id),
        selected.namespace_program_id
    )));
    let output = super::associated_token_account(trader, definition);
    assert!(
        call.account_mentions()
            .iter()
            .any(|mention| mention.actor().account_id == output
                && !mention.authorizes
                && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. }))
    );
}

#[test]
fn fixed_price_exact_output_builder_keeps_membership_and_cap() {
    let membership = curve_core::MembershipPath {
        index: 4,
        salt: [5; 32],
        siblings: [[6; 32]; 17],
    };
    let call = super::build_fixed_price_buy_invocation(
        lbp_sale(),
        AccountId::new([9; 32]),
        ExactOutputTradeTerms {
            amount_out: 10,
            max_amount_in: 25,
        },
        Some(membership.clone()),
        9,
    );
    let lbp_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        lbp_core::Message::BuyExactOutput {
            membership: Some(membership),
            amount_out: 10,
            max_amount_in: 25,
            now: 9
        }
    );
}

fn fixed_creation() -> factory_core::FixedPriceInputs {
    let sale = lbp_sale();
    let factory = factory_core::compute_factory_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt,
    );
    let definition = factory_core::compute_definition_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt,
    );
    let pool = lbp_core::compute_pool_pda(
        sale.namespace,
        sale.lbp_program_id,
        definition,
        sale.collateral_definition,
        factory,
    );
    factory_core::FixedPriceInputs {
        buy_policy: curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
            version: 1,
            namespace: *sale.namespace.value(),
            sale_policy_id: *pool.value(),
            root: [42; 32],
        }),
        pause_policy: curve_core::PausePolicy::Disabled,
        ending_policy: curve_core::EndingPolicy::Either,
        trade_mode: super::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: GraduationPolicy::default(),
        namespace: sale.namespace,
        launch_salt: sale.launch_salt,
        name: "Sale".into(),
        uri: "ipfs://sale".into(),
        graduation_reserve: 0,
        sale_inventory: 800,
        creator_allocation: 200,
        price: lbp_core::FixedPrice {
            collateral_units: 25,
            token_units: 10,
        },
        start: factory_core::SaleStart::Now { timestamp: 9 },
        end_timestamp: Some(100),
        allocation_release: factory_core::CreatorAllocationRelease::AfterSetup,
        namespace_program_id: sale.namespace_program_id,
        lbp_program_id: sale.lbp_program_id,
        collateral_definition_id: sale.collateral_definition,
        creator: AccountId::new([9; 32]),
        now: 9,
    }
}

#[test]
fn fixed_price_creation_builder_preserves_policy_and_admits_setup_accounts() {
    let selected = lbp_sale();
    let inputs = fixed_creation();
    let call =
        super::build_create_fixed_price_invocation(selected.factory_program_id, inputs.clone());
    assert_eq!(
        call.instruction,
        factory_core::Message::CreateFixedPriceLbp {
            inputs: inputs.clone()
        }
    );
    assert_eq!(call.signer_accounts, [inputs.creator]);
    assert_eq!(
        call.destination,
        Actor::new(
            factory_core::compute_factory_pda(
                selected.namespace,
                selected.factory_program_id,
                selected.launch_salt
            ),
            selected.factory_program_id
        )
    );
    let role = factory_core::compute_creator_role_pda(
        selected.namespace,
        selected.factory_program_id,
        selected.launch_salt,
    );
    let mentions = call.account_mentions();
    assert!(mentions.iter().any(|mention| mention.actor()
        == Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        && !mention.authorizes
        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. })));
    assert!(call.actors.contains(&Actor::new(
        inputs.creator,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
    )));
}

#[test]
fn fixed_price_continuation_builder_keeps_stage_and_has_no_replacement_policy() {
    for stage in [
        factory_core::LbpCreationStage::Minted,
        factory_core::LbpCreationStage::Allocated,
        factory_core::LbpCreationStage::PoolPrepared,
    ] {
        let call = super::build_continue_fixed_price_invocation(
            lbp_sale(),
            AccountId::new([9; 32]),
            stage,
            9,
        );
        assert_eq!(
            call.instruction,
            factory_core::Message::ContinueFixedPriceCreation {
                expected_stage: stage,
                creator: AccountId::new([9; 32]),
                now: 9
            }
        );
        let first = super::build_create_fixed_price_invocation(
            lbp_sale().factory_program_id,
            fixed_creation(),
        );
        assert_eq!(call.actors, first.actors);
        assert_eq!(call.pda_accounts, first.pda_accounts);
    }
}

#[test]
fn weighted_exact_input_builder_keeps_membership_limits_and_pricing_clock() {
    let trader = AccountId::new([9; 32]);
    let membership = curve_core::MembershipPath {
        index: 4,
        salt: [5; 32],
        siblings: [[6; 32]; 17],
    };
    let call = super::build_weighted_buy_with_collateral_invocation(
        lbp_sale(),
        trader,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        Some(membership.clone()),
    );
    assert_eq!(
        call.destination,
        Actor::new(trader, lbp_sale().lbp_program_id)
    );
    assert_eq!(call.signer_accounts, [trader]);
    assert!(call.actors.contains(&lbp_core::pricing_clock::actor()));
    let lbp_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        lbp_core::Message::BuyWeightedExactInput {
            membership: Some(membership),
            amount_in: 25,
            min_amount_out: 10
        }
    );
}

#[test]
fn weighted_exact_output_builder_keeps_membership_cap_and_pricing_clock() {
    let membership = curve_core::MembershipPath {
        index: 4,
        salt: [5; 32],
        siblings: [[6; 32]; 17],
    };
    let call = super::build_weighted_buy_invocation(
        lbp_sale(),
        AccountId::new([9; 32]),
        ExactOutputTradeTerms {
            amount_out: 10,
            max_amount_in: 25,
        },
        Some(membership.clone()),
    );
    assert!(call.actors.contains(&lbp_core::pricing_clock::actor()));
    let lbp_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        lbp_core::Message::BuyWeightedExactOutput {
            membership: Some(membership),
            amount_out: 10,
            max_amount_in: 25
        }
    );
}

fn weighted_creation() -> factory_core::WeightedInputs {
    let fixed = fixed_creation();
    factory_core::WeightedInputs {
        buy_policy: fixed.buy_policy,
        pause_policy: curve_core::PausePolicy::AuthorityControlled,
        ending_policy: curve_core::EndingPolicy::Scheduled,
        trade_mode: super::TradeMode::BuyOnly,
        sell_fee_bps: 0,
        graduation_hook: None,
        graduation_policy: GraduationPolicy::default(),
        namespace: fixed.namespace,
        launch_salt: fixed.launch_salt,
        name: fixed.name,
        uri: fixed.uri,
        graduation_reserve: 0,
        sale_inventory: fixed.sale_inventory,
        creator_allocation: fixed.creator_allocation,
        collateral_seed: 100,
        token_weight_start: lbp_core::WEIGHT_TOTAL / 2 + 1,
        token_weight_end: lbp_core::WEIGHT_TOTAL / 2 - 1,
        start_timestamp: 9,
        end_timestamp: 100,
        allocation_release: fixed.allocation_release,
        namespace_program_id: fixed.namespace_program_id,
        lbp_program_id: fixed.lbp_program_id,
        collateral_definition_id: fixed.collateral_definition_id,
        creator: fixed.creator,
    }
}

#[test]
fn weighted_creation_builder_preserves_policy_weights_and_setup_admission() {
    let inputs = weighted_creation();
    let call =
        super::build_create_weighted_invocation(lbp_sale().factory_program_id, inputs.clone());
    assert_eq!(
        call.instruction,
        factory_core::Message::CreateWeightedLbp { inputs }
    );
    let fixed =
        super::build_create_fixed_price_invocation(lbp_sale().factory_program_id, fixed_creation());
    assert_eq!(call.destination, fixed.destination);
    assert_eq!(call.signer_accounts, fixed.signer_accounts);
    assert_eq!(call.actors, fixed.actors);
    assert_eq!(call.pda_accounts, fixed.pda_accounts);
}

#[test]
fn weighted_continuation_builder_keeps_stage_without_time_or_replacement_policy() {
    for stage in [
        factory_core::LbpCreationStage::Minted,
        factory_core::LbpCreationStage::Allocated,
        factory_core::LbpCreationStage::PoolPrepared,
    ] {
        let call =
            super::build_continue_weighted_invocation(lbp_sale(), AccountId::new([9; 32]), stage);
        assert_eq!(
            call.instruction,
            factory_core::Message::ContinueWeightedCreation {
                expected_stage: stage,
                creator: AccountId::new([9; 32])
            }
        );
        let first = super::build_create_weighted_invocation(
            lbp_sale().factory_program_id,
            weighted_creation(),
        );
        assert_eq!(call.actors, first.actors);
        assert_eq!(call.pda_accounts, first.pda_accounts);
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
fn creation_preserves_the_allowlist_policy_on_the_factory_message() {
    let policy = curve_core::BuyPolicy::Allowlist(curve_core::AllowlistPolicy {
        version: 1,
        namespace: *sale().namespace.value(),
        sale_policy_id: *super::factory_pool_addresses(sale()).pool.value(),
        root: [42; 32],
    });
    let mut request = creation();
    request.buy_policy = policy.clone();
    let call = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).unwrap();
    let factory_core::Message::CreateFactoryPool { buy_policy, .. } = call.instruction else {
        panic!("initial creation message");
    };
    assert_eq!(buy_policy, policy);
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
fn resumed_creation_rejects_allowlist_changes_or_downgrade_at_every_stage() {
    let policy = curve_core::AllowlistPolicy {
        version: 1,
        namespace: *sale().namespace.value(),
        sale_policy_id: *super::factory_pool_addresses(sale()).pool.value(),
        root: [42; 32],
    };
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
        CreationStage::Active,
    ] {
        let mut state = valid_factory_state(sale());
        state.buy_policy = curve_core::BuyPolicy::Allowlist(policy.clone());
        state.creation_stage = stage;
        let mut request = creation();
        request.buy_policy = state.buy_policy.clone();
        assert!(
            super::next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        let mut changed_root = policy.clone();
        changed_root.root = [43; 32];
        for replacement in [
            curve_core::BuyPolicy::Open,
            curve_core::BuyPolicy::Allowlist(changed_root),
        ] {
            request.buy_policy = replacement;
            assert!(matches!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    request.clone(),
                    Some(&state)
                ),
                Err(ClientError::LaunchParametersMismatch)
            ));
        }
    }
}

#[test]
fn resumed_creation_rejects_changed_token_metadata() {
    let state = valid_factory_state(sale());
    let mut request = creation();
    request.name = "Changed token".into();
    assert!(matches!(
        super::next_creation_invocation(sale(), AccountId::new([9; 32]), request, Some(&state)),
        Err(ClientError::LaunchParametersMismatch)
    ));
}

#[test]
fn factory_launch_builds_derived_accounts_and_creator_authorization() {
    let creator = AccountId::new([9; 32]);
    let call = build_create_sale_invocation(sale(), creator, creation()).unwrap();
    assert_eq!(
        call.destination,
        Actor::new(
            super::factory_pool_addresses(sale()).factory,
            FACTORY_PROGRAM_ID
        )
    );
    assert_eq!(call.signer_accounts, [creator]);
    let role = factory_core::compute_curve_creator_role_pda(
        sale().namespace,
        FACTORY_PROGRAM_ID,
        sale().launch_salt,
    );
    assert!(
        call.actors
            .contains(&Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID))
    );
    assert!(call.actors.contains(&Actor::new(
        creator,
        curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
    )));
    assert!(call.account_mentions().iter().any(|mention| mention.actor()
        == Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        && !mention.authorizes
        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. })));
}

#[test]
fn factory_creation_admits_derived_launch_accounts_and_canonical_holdings() {
    let creator = AccountId::new([9; 32]);
    let call = build_create_sale_invocation(sale(), creator, creation()).unwrap();
    let a = super::factory_pool_addresses(sale());
    let mentions = call.account_mentions();
    for account in [
        a.factory,
        a.token_definition,
        a.pool,
        factory_core::compute_mint_pda(
            sale().namespace,
            sale().factory_program_id,
            sale().launch_salt,
        ),
        super::associated_token_account(
            factory_core::compute_mint_pda(
                sale().namespace,
                sale().factory_program_id,
                sale().launch_salt,
            ),
            a.token_definition,
        ),
        super::associated_token_account(a.creator_escrow, a.token_definition),
        factory_core::compute_curve_creator_role_pda(
            sale().namespace,
            FACTORY_PROGRAM_ID,
            sale().launch_salt,
        ),
    ] {
        let mention = mentions
            .iter()
            .find(|m| m.actor().account_id == account)
            .unwrap();
        assert!(!mention.authorizes);
        assert!(
            matches!(&mention.identity, wallet::AccountIdentity::PublicPda { .. }),
            "{account}"
        );
    }
}

#[test]
fn settlement_admits_escrow_and_current_creator_and_treasury_holdings() {
    let creator = AccountId::new([9; 32]);
    let treasury = AccountId::new([10; 32]);
    let call = build_withdraw_factory_proceeds_invocation(sale(), creator, treasury, 0);
    let a = super::factory_pool_addresses(sale());
    let mentions = call.account_mentions();
    for account in [
        a.creator_escrow,
        super::associated_token_account(creator, sale().collateral_definition),
        super::associated_token_account(treasury, sale().collateral_definition),
    ] {
        let mention = mentions
            .iter()
            .find(|m| m.actor().account_id == account)
            .unwrap();
        assert!(!mention.authorizes);
        assert!(matches!(
            &mention.identity,
            wallet::AccountIdentity::PublicPda { .. }
        ));
    }
}

#[test]
fn allocation_claim_admits_the_creator_output_holding() {
    let creator = AccountId::new([9; 32]);
    let call = build_claim_creator_allocation_invocation(sale(), creator, 0);
    let holding = super::associated_token_account(
        creator,
        super::factory_pool_addresses(sale()).token_definition,
    );
    assert!(
        call.account_mentions()
            .iter()
            .any(|m| m.actor().account_id == holding
                && !m.authorizes
                && matches!(m.identity, wallet::AccountIdentity::PublicPda { .. }))
    );
}

#[test]
fn graduation_admits_adapter_custody_and_its_payout_holdings_without_authorization() {
    let mut state = valid_factory_state(sale());
    let adapter = AccountId::new([12; 32]);
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: adapter,
        configuration: vec![],
    });
    let call = super::build_graduation_invocation(
        super::GraduationTarget::Curve {
            sale: sale(),
            state: &state,
            creator_hint: AccountId::new([9; 32]),
        },
        AccountId::new([10; 32]),
    )
    .unwrap();
    let a = super::factory_pool_addresses(sale());
    let custody = factory_core::compute_graduation_pda(adapter, a.factory);
    for account in [
        custody,
        super::associated_token_account(custody, a.token_definition),
        super::associated_token_account(custody, sale().collateral_definition),
    ] {
        assert!(
            call.account_mentions()
                .iter()
                .any(|m| m.actor().account_id == account
                    && !m.authorizes
                    && matches!(m.identity, wallet::AccountIdentity::PublicPda { .. }))
        );
    }
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
    let mut other = sale();
    other.namespace = AccountId::new([0xAE; 32]);
    let terms = ExactOutputTradeTerms {
        amount_out: 10,
        max_amount_in: 100,
    };
    let first = build_buy_invocation(sale(), AccountId::new([3; 32]), terms, None, 0);
    let second = build_buy_invocation(other, AccountId::new([3; 32]), terms, None, 0);
    assert_ne!(first.instruction, second.instruction);
    assert!(first.actors.contains(&Actor::new(
        curve_core::compute_config_pda(sale().namespace, CURVE_PROGRAM_ID),
        CURVE_PROGRAM_ID
    )));
    assert!(second.actors.contains(&Actor::new(
        curve_core::compute_config_pda(other.namespace, CURVE_PROGRAM_ID),
        CURVE_PROGRAM_ID
    )));
}

#[test]
fn creation_and_trade_builders_use_one_factory_sale_context() {
    let participant = AccountId::new([9; 32]);
    let exact_output = build_buy_invocation(
        sale(),
        participant,
        ExactOutputTradeTerms {
            amount_out: 25,
            max_amount_in: 100,
        },
        None,
        0,
    );
    let exact_input = build_buy_with_collateral_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        None,
        0,
    );
    let sell = build_sell_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        0,
    );
    assert_eq!(
        exact_output.destination,
        Actor::new(participant, CURVE_PROGRAM_ID)
    );
    assert_eq!(exact_output.destination, exact_input.destination);
    assert_eq!(sell.destination, exact_input.destination);
    assert_eq!(exact_output.signer_accounts, [participant]);
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
        matches!(creation.instruction, super::token::Message::Create { kind: super::token::TokenKind::NftMaster, supply: 1, recipient, .. } if recipient == holder)
    );
    let transfer = build_transfer_authority_invocation(holder, recipient, definition);
    assert_eq!(transfer.signer_accounts, [holder]);
    assert_eq!(
        transfer.instruction,
        super::token::Message::Transfer {
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

#[test]
fn factory_instruction_wire_slots_follow_the_source_idl_order() {
    for (message, expected) in [
        (
            factory_core::Message::ContinueCreation {
                creator: AccountId::new([1; 32]),
                now: 0,
            },
            1,
        ),
        (
            factory_core::Message::ClaimCreatorAllocation {
                creator: AccountId::new([1; 32]),
                now: 0,
            },
            2,
        ),
        (
            factory_core::Message::WithdrawFactoryProceeds {
                creator: AccountId::new([1; 32]),
                now: 0,
            },
            3,
        ),
    ] {
        assert_eq!(borsh::to_vec(&message).unwrap()[0], expected);
    }
}

#[test]
fn creator_claim_uses_factory_escrow_and_derived_recipient_ata() {
    let creator = AccountId::new([9; 32]);
    let a = super::factory_pool_addresses(sale());
    let call = build_claim_creator_allocation_invocation(sale(), creator, 0);
    assert_eq!(call.destination, Actor::new(a.factory, FACTORY_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [creator]);
    assert!(
        call.actors
            .contains(&super::token_actor(super::associated_token_account(
                a.creator_escrow,
                a.token_definition
            )))
    );
    assert!(
        call.actors
            .contains(&super::token_actor(super::associated_token_account(
                creator,
                a.token_definition
            )))
    );
}

#[test]
fn saved_settlement_stages_identify_completed_manual_work() {
    for (stage, complete) in [
        (SettlementStage::Complete, true),
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
        (
            SettlementStage::HookReady {
                unsold: 0,
                collateral_due: 0,
            },
            false,
        ),
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
fn settlement_builders_use_factory_actor_and_creator_signature() {
    let creator = AccountId::new([9; 32]);
    let claim = build_claim_creator_allocation_invocation(sale(), creator, 0);
    let withdrawal =
        build_withdraw_factory_proceeds_invocation(sale(), creator, AccountId::new([4; 32]), 0);
    assert_eq!(claim.destination, withdrawal.destination);
    assert_eq!(claim.signer_accounts, [creator]);
    assert_eq!(withdrawal.signer_accounts, [creator]);
}

#[test]
fn exact_output_purchase_preparation_keeps_membership_and_price_limits() {
    let (pool, config) = pool_snapshot();
    let membership = curve_core::MembershipPath {
        index: 3,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let call = super::prepare_buy_invocation(
        sale(),
        AccountId::new([9; 32]),
        &pool,
        &config,
        ExactOutputTradeTerms {
            amount_out: 200,
            max_amount_in: 25,
        },
        Some(membership.clone()),
        1,
    )
    .unwrap();
    let curve_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        curve_core::Message::SwapExactOutput {
            amount_out: 200,
            max_amount_in: 25,
            token_in: sale().collateral_definition,
            membership: Some(membership),
            now: 1,
        }
    );
}

#[test]
fn exact_output_buy_uses_collateral_input_and_typed_participant_limits() {
    let participant = AccountId::new([9; 32]);
    let call = build_buy_invocation(
        sale(),
        participant,
        ExactOutputTradeTerms {
            amount_out: 25,
            max_amount_in: 100,
        },
        None,
        0,
    );
    assert_eq!(call.destination, Actor::new(participant, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [participant]);
    let curve_core::Message::Execute { pool, instruction } = call.instruction else {
        panic!("signed trade envelope");
    };
    assert_eq!(pool, super::factory_pool_addresses(sale()).pool);
    assert!(
        matches!(*instruction, curve_core::Message::SwapExactOutput { amount_out: 25, max_amount_in: 100, token_in, membership: None, now: 0 } if token_in == sale().collateral_definition)
    );
}

#[test]
fn collateral_buy_uses_collateral_input_and_typed_participant_limits() {
    let participant = AccountId::new([9; 32]);
    let call = build_buy_with_collateral_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 100,
        },
        None,
        0,
    );
    assert_eq!(call.destination, Actor::new(participant, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [participant]);
    let curve_core::Message::Execute { pool, instruction } = call.instruction else {
        panic!("signed trade envelope");
    };
    assert_eq!(pool, super::factory_pool_addresses(sale()).pool);
    assert!(
        matches!(*instruction, curve_core::Message::SwapExactInput { amount_in: 25, min_amount_out: 100, token_in, membership: None, now: 0 } if token_in == sale().collateral_definition)
    );
}

#[test]
fn sell_uses_launch_token_input_and_typed_participant_limits() {
    let participant = AccountId::new([9; 32]);
    let call = build_sell_invocation(
        sale(),
        participant,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 10,
        },
        0,
    );
    assert_eq!(call.destination, Actor::new(participant, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [participant]);
    let curve_core::Message::Execute { pool, instruction } = call.instruction else {
        panic!("signed trade envelope");
    };
    assert_eq!(pool, super::factory_pool_addresses(sale()).pool);
    assert!(
        matches!(*instruction, curve_core::Message::SwapExactInput { amount_in: 25, min_amount_out: 10, token_in, membership: None, now: 0 } if token_in == super::factory_pool_addresses(sale()).token_definition)
    );
}

#[test]
fn collateral_purchase_preparation_keeps_membership_and_price_limits() {
    let (pool, config) = pool_snapshot();
    let membership = curve_core::MembershipPath {
        index: 0x01020304,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let call = super::prepare_buy_with_collateral_invocation(
        sale(),
        AccountId::new([9; 32]),
        &pool,
        &config,
        ExactInputTradeTerms {
            amount_in: 25,
            min_amount_out: 200,
        },
        Some(membership.clone()),
        1,
    )
    .unwrap();
    let curve_core::Message::Execute { instruction, .. } = call.instruction else {
        panic!("signed purchase");
    };
    assert_eq!(
        *instruction,
        curve_core::Message::SwapExactInput {
            amount_in: 25,
            min_amount_out: 200,
            token_in: sale().collateral_definition,
            membership: Some(membership),
            now: 1,
        }
    );
}

#[test]
fn collateral_purchase_preparation_rejects_output_below_participant_floor() {
    let (pool, config) = pool_snapshot();
    assert!(matches!(
        super::prepare_buy_with_collateral_invocation(
            sale(),
            AccountId::new([9; 32]),
            &pool,
            &config,
            ExactInputTradeTerms {
                amount_in: 25,
                min_amount_out: 201
            },
            None,
            1
        ),
        Err(ClientError::QuoteBuyWithCollateral(
            pool::SwapError::SlippageExceeded
        ))
    ));
}

#[test]
fn sale_preparation_rejects_output_below_participant_floor() {
    let (pool, config) = pool_snapshot();
    assert!(matches!(
        super::prepare_sell_invocation(
            sale(),
            AccountId::new([9; 32]),
            &pool,
            &config,
            ExactInputTradeTerms {
                amount_in: 250,
                min_amount_out: 21
            },
            1
        ),
        Err(ClientError::QuoteSell(pool::SwapError::SlippageExceeded))
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
    let creator = AccountId::new([2; 32]);
    let pool = AccountId::new([4; 32]);
    let collection = build_collect_fees_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        pool,
        sale().collateral_definition,
        creator,
    );
    assert!(collection.signer_accounts.is_empty());
    assert_eq!(collection.destination, Actor::new(pool, CURVE_PROGRAM_ID));
    assert!(
        collection
            .actors
            .contains(&super::token_actor(super::associated_token_account(
                creator,
                sale().collateral_definition
            )))
    );
    let withdrawal = build_withdraw_factory_proceeds_invocation(sale(), creator, creator, 0);
    assert_eq!(
        withdrawal.actors.len(),
        withdrawal
            .actors
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
}

#[test]
fn collection_builder_omits_the_treasury_when_it_is_the_pool_vault() {
    let pool = AccountId::new([4; 32]);
    let call = build_collect_fees_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        pool,
        sale().collateral_definition,
        pool,
    );
    assert_eq!(
        call.actors.len(),
        call.actors
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
    );
    assert!(call.signer_accounts.is_empty());
    assert_eq!(call.instruction, curve_core::Message::CollectFees);
}

#[test]
fn graduation_builder_uses_typed_callback_and_adapter_custody() {
    let mut state = valid_factory_state(sale());
    let adapter = AccountId::new([9; 32]);
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: adapter,
        configuration: vec![7, 11],
    });
    let call = super::build_graduation_invocation(
        super::GraduationTarget::Curve {
            sale: sale(),
            state: &state,
            creator_hint: AccountId::new([9; 32]),
        },
        AccountId::new([4; 32]),
    )
    .unwrap();
    assert!(call.signer_accounts.is_empty());
    assert!(call.actors.contains(&Actor::new(
        factory_core::compute_graduation_pda(
            adapter,
            super::factory_pool_addresses(sale()).factory
        ),
        adapter
    )));
    assert_eq!(
        call.instruction,
        factory_core::Message::ExecuteGraduationHook {
            creator_hint: Some(AccountId::new([9; 32]))
        }
    );
}

#[test]
fn factory_withdrawal_routes_reserves_through_factory_policy() {
    let creator = AccountId::new([9; 32]);
    let a = super::factory_pool_addresses(sale());
    let call =
        build_withdraw_factory_proceeds_invocation(sale(), creator, AccountId::new([4; 32]), 0);
    assert_eq!(call.destination, Actor::new(a.factory, FACTORY_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [creator]);
    assert_eq!(
        call.instruction,
        factory_core::Message::WithdrawFactoryProceeds { creator, now: 0 }
    );
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

#[test]
fn factory_state_rejects_a_mismatched_creator_role() {
    let mut state = valid_factory_state(sale());
    state.creator_role_id = AccountId::new([94; 32]);
    assert!(matches!(
        validate_factory_state(sale(), &state),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn creation_and_continuations_admit_the_factory_curve_relay_without_authorization() {
    let selected = sale();
    let factory = factory_core::compute_factory_pda(
        selected.namespace,
        selected.factory_program_id,
        selected.launch_salt,
    );
    for stage in [
        None,
        Some(CreationStage::Minted),
        Some(CreationStage::Allocated),
        Some(CreationStage::PoolPrepared),
    ] {
        let state = stage.map(|stage| {
            let mut state = valid_factory_state(selected);
            state.creation_stage = stage;
            state
        });
        let call = super::next_creation_invocation(
            selected,
            AccountId::new([9; 32]),
            creation(),
            state.as_ref(),
        )
        .unwrap()
        .unwrap();
        let mentions = call.account_mentions();
        let relay = mentions
            .iter()
            .find(|mention| mention.actor() == Actor::new(factory, selected.curve_program_id))
            .expect("factory curve relay");
        assert!(!relay.authorizes);
        assert!(matches!(relay.identity,
            wallet::AccountIdentity::PublicPda { program, seed }
                if program == selected.factory_program_id
                    && seed == factory_core::compute_factory_seed(selected.namespace, selected.launch_salt)
        ));
        assert_eq!(call.message_bytes().unwrap()[0], u8::from(stage.is_some()));
    }
}

#[test]
fn pause_builders_keep_current_role_signers_and_factory_relay_admission() {
    let creator = AccountId::new([9; 32]);
    let curve = super::build_set_factory_paused_invocation(sale(), creator, true);
    let lbp = super::build_set_lbp_paused_invocation(lbp_sale(), creator, true);
    for (call, namespace, salt, factory_program, engine, role) in [
        (
            &curve,
            sale().namespace,
            sale().launch_salt,
            sale().factory_program_id,
            sale().curve_program_id,
            factory_core::compute_curve_creator_role_pda(
                sale().namespace,
                sale().factory_program_id,
                sale().launch_salt,
            ),
        ),
        (
            &lbp,
            lbp_sale().namespace,
            lbp_sale().launch_salt,
            lbp_sale().factory_program_id,
            lbp_sale().lbp_program_id,
            factory_core::compute_creator_role_pda(
                lbp_sale().namespace,
                lbp_sale().factory_program_id,
                lbp_sale().launch_salt,
            ),
        ),
    ] {
        let factory = factory_core::compute_factory_pda(namespace, factory_program, salt);
        assert_eq!(call.destination, Actor::new(factory, factory_program));
        assert_eq!(call.signer_accounts, [creator]);
        assert_eq!(
            call.instruction,
            factory_core::Message::SetPaused {
                creator,
                paused: true
            }
        );
        assert!(call.actors.contains(&Actor::new(
            creator,
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
        )));
        for actor in [
            Actor::new(factory, engine),
            Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        ] {
            assert!(
                call.account_mentions()
                    .iter()
                    .any(|mention| mention.actor() == actor
                        && !mention.authorizes
                        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. }))
            );
        }
    }
}

#[test]
fn direct_pause_builder_uses_owner_role_and_signed_curve_envelope() {
    let owner = AccountId::new([9; 32]);
    let pool = AccountId::new([10; 32]);
    let call = super::build_set_curve_paused_invocation(
        sale().namespace,
        CURVE_PROGRAM_ID,
        pool,
        owner,
        false,
    );
    assert_eq!(call.destination, Actor::new(owner, CURVE_PROGRAM_ID));
    assert_eq!(call.signer_accounts, [owner]);
    assert_eq!(
        call.instruction,
        curve_core::Message::Execute {
            pool,
            instruction: Box::new(curve_core::Message::SetPaused { paused: false })
        }
    );
    let role = curve_core::compute_pool_owner_role(sale().namespace, pool, CURVE_PROGRAM_ID);
    assert!(call.account_mentions().iter().any(|mention| mention.actor()
        == Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        && !mention.authorizes
        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. })));
    assert!(call.actors.contains(&super::clock_actor()));
}

#[test]
fn creation_preserves_pause_policy_and_resumption_rejects_changes_at_every_stage() {
    for policy in [
        curve_core::PausePolicy::Disabled,
        curve_core::PausePolicy::AuthorityControlled,
    ] {
        let mut request = creation();
        request.pause_policy = policy;
        let call =
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()).unwrap();
        assert!(
            matches!(call.instruction, factory_core::Message::CreateFactoryPool { pause_policy, .. } if pause_policy == policy)
        );
        for stage in [
            CreationStage::Minted,
            CreationStage::Allocated,
            CreationStage::PoolPrepared,
            CreationStage::Active,
        ] {
            let mut state = valid_factory_state(sale());
            state.creation_stage = stage;
            state.pause_policy = policy;
            assert!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    request.clone(),
                    Some(&state)
                )
                .is_ok()
            );
            state.pause_policy = match policy {
                curve_core::PausePolicy::Disabled => curve_core::PausePolicy::AuthorityControlled,
                curve_core::PausePolicy::AuthorityControlled => curve_core::PausePolicy::Disabled,
            };
            assert!(matches!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    request.clone(),
                    Some(&state)
                ),
                Err(ClientError::LaunchParametersMismatch)
            ));
        }
    }
}

#[test]
fn paused_curve_quotes_reject_both_buy_forms_and_sells() {
    let (mut pool, config) = pool_snapshot();
    pool.trade_control = curve_core::TradeControl::Paused;
    for result in [
        quote_buy(&pool, &config, 1, 1),
        quote_buy_with_collateral(&pool, &config, 1, 1),
        quote_sell(&pool, &config, 1, 1),
    ] {
        assert!(matches!(result, Err(ClientError::PoolPaused)));
    }
}

#[test]
fn creation_accepts_only_factory_ending_windows() {
    use curve_core::EndingPolicy::{Depletion, Either, Manual, Scheduled};
    for ending in [Manual, Scheduled, Depletion, Either] {
        for end in [None, Some(100)] {
            let mut request = creation();
            request.ending_policy = ending;
            request.end_timestamp = end;
            let result = build_create_sale_invocation(sale(), AccountId::new([9; 32]), request);
            let supported = matches!(
                (ending, end),
                (Depletion, None) | (Scheduled | Either, Some(_))
            );
            if supported {
                assert!(matches!(
                    result.unwrap().instruction,
                    factory_core::Message::CreateFactoryPool { ending_policy, end_timestamp, .. }
                        if ending_policy == ending && end_timestamp == end
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(ClientError::FactoryPolicy(
                        factory_core::Error::InvalidTerms
                    ))
                ));
            }
        }
    }
}

#[test]
fn creation_resume_rejects_ending_and_deadline_replacements_at_every_stage() {
    use curve_core::EndingPolicy::{Depletion, Either, Scheduled};
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
        CreationStage::Active,
    ] {
        for (ending, end) in [
            (Depletion, None),
            (Scheduled, Some(100)),
            (Either, Some(100)),
        ] {
            let mut state = valid_factory_state(sale());
            state.creation_stage = stage;
            state.ending_policy = ending;
            state.end_timestamp = end;
            let mut request = creation();
            request.ending_policy = ending;
            request.end_timestamp = end;
            assert!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    request.clone(),
                    Some(&state)
                )
                .is_ok()
            );
            for (replacement, deadline) in [
                (Depletion, None),
                (Scheduled, Some(100)),
                (Either, Some(100)),
                (ending, Some(101)),
            ] {
                if (replacement, deadline) == (ending, end) {
                    continue;
                }
                let mut changed = request.clone();
                changed.ending_policy = replacement;
                changed.end_timestamp = deadline;
                assert!(matches!(
                    super::next_creation_invocation(
                        sale(),
                        AccountId::new([9; 32]),
                        changed,
                        Some(&state)
                    ),
                    Err(ClientError::LaunchParametersMismatch)
                ));
            }
        }
    }
}

#[test]
fn factory_state_rejects_invalid_ending_windows() {
    use curve_core::EndingPolicy::{Depletion, Either, Manual, Scheduled};
    for (ending, end) in [
        (Manual, None),
        (Manual, Some(100)),
        (Scheduled, None),
        (Either, None),
        (Depletion, Some(100)),
    ] {
        let mut state = valid_factory_state(sale());
        state.ending_policy = ending;
        state.end_timestamp = end;
        assert!(matches!(
            super::validate_factory_state(sale(), &state),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
}

fn matching_factory_pool(state: &FactoryState) -> curve_core::PoolAccount {
    let (mut pool, _) = pool_snapshot();
    pool.namespace = state.namespace;
    pool.token0_definition_id = state.token_definition_id;
    pool.token1_definition_id = state.collateral_definition_id;
    pool.owner = super::factory_pool_addresses(sale()).factory;
    pool.owner_program = Some((
        sale().factory_program_id,
        *factory_core::compute_factory_seed(state.namespace, state.launch_salt).as_bytes(),
    ));
    pool.pool.virtual_reserve0 = state.virtual_token_reserve;
    pool.pool.virtual_reserve1 = state.virtual_collateral_reserve;
    pool.pool.initial_k = state.virtual_token_reserve * state.virtual_collateral_reserve;
    pool.pool.k = pool.pool.initial_k;
    pool.pool.sell_fee_bps = state.sell_fee_bps;
    pool.pool.ending_policy = state.ending_policy;
    pool.pool.close_timestamp = state.end_timestamp;
    pool.pool.close_on_depletion = match state.ending_policy {
        curve_core::EndingPolicy::Scheduled => None,
        _ => Some(pool::TokenSide::Token0),
    };
    pool
}

#[test]
fn loaded_factory_pool_checks_all_immutable_policy_and_pricing_terms() {
    let state = valid_factory_state(sale());
    let pool = matching_factory_pool(&state);
    super::validate_factory_pool(sale(), &state, &pool).unwrap();
    for change in 1..10 {
        let mut changed = pool.clone();
        match change {
            1 => changed.pool.ending_policy = curve_core::EndingPolicy::Manual,
            2 => changed.pool.close_timestamp = Some(100),
            3 => changed.pool.close_on_depletion = Some(pool::TokenSide::Token1),
            4 => changed.allowed_input_token = Some(pool::TokenSide::Token1),
            5 => changed.trade_control = curve_core::TradeControl::Running,
            6 => changed.pool.initial_k += 1,
            7 => changed.owner_program = None,
            8 => changed.token0_definition_id = AccountId::new([99; 32]),
            9 => changed.buy_policy = fixed_creation().buy_policy,
            _ => unreachable!(),
        }
        assert!(
            matches!(
                super::validate_factory_pool(sale(), &state, &changed),
                Err(ClientError::FactorySaleMismatch)
            ),
            "change {change}"
        );
    }
    let mut paused_state = state.clone();
    paused_state.pause_policy = curve_core::PausePolicy::AuthorityControlled;
    let mut paused_pool = pool.clone();
    paused_pool.trade_control = curve_core::TradeControl::Paused;
    super::validate_factory_pool(sale(), &paused_state, &paused_pool).unwrap();
    let mut traded_pool = pool;
    traded_pool.pool.virtual_reserve0 -= 1;
    traded_pool.pool.virtual_reserve1 += 1;
    traded_pool.pool.real_reserve0 -= 1;
    traded_pool.pool.real_reserve1 += 1;
    super::validate_factory_pool(sale(), &state, &traded_pool).unwrap();
}

#[test]
fn malformed_policy_quotes_keep_the_quote_error_label_and_source() {
    use std::error::Error;
    let (original, config) = pool_snapshot();
    for change in 0..4 {
        let mut pool = original.clone();
        match change {
            0 => pool.pool.ending_policy = curve_core::EndingPolicy::Either,
            1 => pool.pool.ending_policy = curve_core::EndingPolicy::Scheduled,
            2 => pool.pool.ending_policy = curve_core::EndingPolicy::Depletion,
            3 => pool.pool.close_timestamp = Some(100),
            _ => unreachable!(),
        }
        let before = pool.clone();
        for (error, label) in [
            (
                super::quote_buy(&pool, &config, 1, 0).unwrap_err(),
                "quote_buy",
            ),
            (
                super::quote_sell(&pool, &config, 1, 0).unwrap_err(),
                "quote_sell",
            ),
            (
                super::quote_buy_with_collateral(&pool, &config, 1, 0).unwrap_err(),
                "quote_buy_with_collateral",
            ),
        ] {
            let actual: &'static str = (&error).into();
            assert_eq!(actual, label);
            assert!(matches!(
                error.source().unwrap().downcast_ref::<pool::SwapError>(),
                Some(pool::SwapError::InvalidPolicy)
            ));
        }
        assert_eq!(pool, before);
    }
}

#[test]
fn loaded_factory_pool_matches_each_selected_ending() {
    use curve_core::EndingPolicy::{Depletion, Either, Scheduled};
    for (ending, end) in [
        (Depletion, None),
        (Scheduled, Some(100)),
        (Either, Some(100)),
    ] {
        let mut state = valid_factory_state(sale());
        state.ending_policy = ending;
        state.end_timestamp = end;
        let pool = matching_factory_pool(&state);
        super::validate_factory_pool(sale(), &state, &pool).unwrap();
        for side in [
            None,
            Some(pool::TokenSide::Token0),
            Some(pool::TokenSide::Token1),
        ] {
            let mut changed = pool.clone();
            changed.pool.close_on_depletion = side;
            assert_eq!(
                super::validate_factory_pool(sale(), &state, &changed).is_ok(),
                side == pool.pool.close_on_depletion
            );
        }
        let mut changed = pool;
        changed.pool.ending_policy = if ending == Either { Scheduled } else { Either };
        assert!(matches!(
            super::validate_factory_pool(sale(), &state, &changed),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
}

fn valid_lbp_factory_state(weighted: bool) -> factory_core::LbpLaunchState {
    let sale = lbp_sale();
    let a = super::lbp_addresses(sale);
    let mut bytes = Vec::new();
    macro_rules! field {
        ($value:expr) => {
            bytes.extend(borsh::to_vec(&$value).unwrap());
        };
    }
    field!(1000u16);
    field!(curve_core::BuyPolicy::Open);
    field!(curve_core::PausePolicy::Disabled);
    field!(if weighted {
        curve_core::EndingPolicy::Scheduled
    } else {
        curve_core::EndingPolicy::Either
    });
    field!(super::TradeMode::BuyOnly);
    field!(0u16);
    field!(Some(factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![7, 11]
    }));
    field!(GraduationPolicy::default());
    field!(if weighted { *b"LZWGLAUN" } else { *b"LZFXLAUN" });
    field!(6u16);
    field!(factory_core::compute_creator_role_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt
    ));
    field!(if weighted {
        factory_core::LbpSetupMode::Weighted {
            collateral_seed: 100,
            token_weight_start: lbp_core::WEIGHT_TOTAL / 2 + 1,
            token_weight_end: lbp_core::WEIGHT_TOTAL / 2 - 1,
        }
    } else {
        factory_core::LbpSetupMode::FixedPrice
    });
    field!(factory_core::LbpCreationStage::Complete);
    field!(lbp_core::SettlementProgress::Unselected);
    field!(sale.namespace);
    field!(sale.launch_salt);
    field!(a.token_definition);
    field!(sale.collateral_definition);
    field!(a.creator_escrow);
    field!(a.pool);
    field!(sale.namespace_program_id);
    field!(sale.lbp_program_id);
    field!(800u128);
    field!(0u128); // Graduation reserve.
    field!(200u128);
    field!(1_000u128);
    field!(if weighted {
        lbp_core::FixedPrice {
            collateral_units: 0,
            token_units: 0,
        }
    } else {
        lbp_core::FixedPrice {
            collateral_units: 25,
            token_units: 10,
        }
    });
    field!(9u64);
    field!(Some(100u64));
    field!(factory_core::CreatorAllocationRelease::AfterEnd);
    field!(false);
    field!("Sale".to_owned());
    field!("ipfs://sale".to_owned());
    field!(None::<u8>);
    field!(None::<u8>);
    factory_core::LbpLaunchState::decode(&bytes).unwrap()
}

#[test]
fn lbp_graduation_admits_setup_callback_and_custody_without_creator_authorization() {
    let sale = lbp_sale();
    let a = super::lbp_addresses(sale);
    for weighted in [false, true] {
        let state = valid_lbp_factory_state(weighted);
        for creator_hint in [AccountId::new([21; 32]), AccountId::new([22; 32])] {
            let treasury = AccountId::new([23; 32]);
            let call = super::build_graduation_invocation(
                super::GraduationTarget::Lbp {
                    sale,
                    state: &state,
                    creator_hint,
                },
                treasury,
            )
            .unwrap();
            assert_eq!(
                call.destination,
                Actor::new(a.factory, sale.factory_program_id)
            );
            assert!(call.signer_accounts.is_empty());
            assert_eq!(
                call.instruction,
                factory_core::Message::ExecuteGraduationHook {
                    creator_hint: Some(creator_hint)
                }
            );
            assert!(
                call.account_mentions()
                    .iter()
                    .all(|mention| !mention.authorizes)
            );
            let custody_program = state.graduation_hook.as_ref().unwrap().program_id;
            let custody = factory_core::compute_graduation_pda(custody_program, a.factory);
            for actor in [
                Actor::new(
                    state.creator_role_id,
                    curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID,
                ),
                Actor::new(creator_hint, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
                Actor::new(a.factory, sale.lbp_program_id),
                Actor::new(a.pool, sale.lbp_program_id),
                Actor::new(custody, custody_program),
                Actor::new(
                    curve_core::compute_config_pda(sale.namespace, sale.namespace_program_id),
                    sale.namespace_program_id,
                ),
                super::clock_actor(),
                super::token_actor(a.token_definition),
                super::token_actor(sale.collateral_definition),
            ] {
                assert!(call.actors.contains(&actor), "missing {actor:?}");
            }
            let mentions = call.account_mentions();
            for owner in [
                a.factory,
                a.pool,
                a.creator_escrow,
                creator_hint,
                custody,
                treasury,
            ] {
                for definition in [a.token_definition, sale.collateral_definition] {
                    let holding =
                        super::token_actor(super::associated_token_account(owner, definition));
                    assert!(mentions.iter().any(|m| m.actor() == holding
                        && !m.authorizes
                        && matches!(m.identity, wallet::AccountIdentity::PublicPda { .. })));
                }
            }
            assert_eq!(
                call.actors.len(),
                call.actors
                    .iter()
                    .collect::<std::collections::HashSet<_>>()
                    .len()
            );
        }
    }
}

#[test]
fn lbp_graduation_rejects_foreign_identity_and_requires_a_saved_hook() {
    let sale = lbp_sale();
    let hint = AccountId::new([21; 32]);
    let treasury = AccountId::new([23; 32]);
    for weighted in [false, true] {
        let state = valid_lbp_factory_state(weighted);
        for index in 0..9 {
            let mut bad = state.clone();
            let foreign = AccountId::new([99; 32]);
            match index {
                0 => bad.namespace = foreign,
                1 => bad.launch_salt = [99; 32],
                2 => bad.namespace_program_id = foreign,
                3 => bad.lbp_program_id = foreign,
                4 => bad.collateral_definition_id = foreign,
                5 => bad.token_definition_id = foreign,
                6 => bad.creator_escrow_id = foreign,
                7 => bad.pool_id = foreign,
                _ => bad.creator_role_id = foreign,
            }
            assert!(matches!(
                super::build_graduation_invocation(
                    super::GraduationTarget::Lbp {
                        sale,
                        state: &bad,
                        creator_hint: hint,
                    },
                    treasury
                ),
                Err(ClientError::FactorySaleMismatch)
            ));
        }
        let mut no_hook = state;
        no_hook.graduation_hook = None;
        assert!(matches!(
            super::build_graduation_invocation(
                super::GraduationTarget::Lbp {
                    sale,
                    state: &no_hook,
                    creator_hint: hint,
                },
                treasury
            ),
            Err(ClientError::GraduationNotConfigured)
        ));
    }
}

#[test]
fn typed_lbp_creation_keeps_the_complete_immutable_hook() {
    let hook = factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![3, 7, 11],
    };
    let mut fixed = fixed_creation();
    fixed.graduation_hook = Some(hook.clone());
    let fixed_call =
        super::build_create_fixed_price_invocation(lbp_sale().factory_program_id, fixed.clone());
    assert_eq!(
        fixed_call.instruction,
        factory_core::Message::CreateFixedPriceLbp { inputs: fixed }
    );
    let mut weighted = weighted_creation();
    weighted.graduation_hook = Some(hook);
    let weighted_call =
        super::build_create_weighted_invocation(lbp_sale().factory_program_id, weighted.clone());
    assert_eq!(
        weighted_call.instruction,
        factory_core::Message::CreateWeightedLbp { inputs: weighted }
    );
}

#[test]
fn curve_resume_rejects_hook_program_configuration_and_presence_changes_at_each_stage() {
    let hook = factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![3, 7],
    };
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
        CreationStage::Active,
    ] {
        let mut state = valid_factory_state(sale());
        state.creation_stage = stage;
        state.graduation_hook = Some(hook.clone());
        let mut request = creation();
        request.graduation_hook = Some(hook.clone());
        assert!(
            super::next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        for changed in [
            None,
            Some(factory_core::GraduationHook {
                program_id: AccountId::new([20; 32]),
                ..hook.clone()
            }),
            Some(factory_core::GraduationHook {
                configuration: vec![3, 8],
                ..hook.clone()
            }),
        ] {
            request.graduation_hook = changed;
            assert!(matches!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    request.clone(),
                    Some(&state)
                ),
                Err(ClientError::LaunchParametersMismatch)
            ));
        }
    }
}

fn lbp_pool_snapshot(weighted: bool) -> (lbp_core::PoolAccount, curve_core::Config) {
    let sale = lbp_sale();
    let a = super::lbp_addresses(sale);
    let mode = if weighted {
        lbp_core::PoolMode::Weighted {
            token_weight_start: lbp_core::WEIGHT_TOTAL / 2,
            token_weight_end: lbp_core::WEIGHT_TOTAL / 2,
            collateral_seed: 100,
            pricing_collateral: 1_000,
            checkpoint_time: 9,
            checkpoint_token_weight: lbp_core::WEIGHT_TOTAL / 2,
        }
    } else {
        lbp_core::PoolMode::FixedPrice
    };
    let mut bytes = borsh::to_vec(&(
        1000u16,
        curve_core::BuyPolicy::Open,
        curve_core::TradeControl::Unrestricted,
        curve_core::EndingPolicy::Scheduled,
        super::TradeMode::BuyAndSell,
        0u16,
        None::<Actor>,
        GraduationPolicy::default(),
        mode,
        (true, false),
        (
            sale.namespace,
            sale.namespace_program_id,
            a.factory,
            sale.factory_program_id,
            *factory_core::compute_factory_seed(sale.namespace, sale.launch_salt).as_bytes(),
            a.token_definition,
            sale.collateral_definition,
        ),
        (800u128, 700u128, 200u128),
        lbp_core::FixedPrice {
            collateral_units: 2,
            token_units: 1,
        },
        (
            9u64,
            Some(100u64),
            lbp_core::CreatorAllocationRelease::AfterEnd,
        ),
        (1_000u128, 0u128, lbp_core::SettlementProgress::Unselected),
    ))
    .unwrap();
    bytes.push(0);
    let pool = lbp_core::PoolAccount::decode(&bytes).unwrap();
    let (_, mut config) = pool_snapshot();
    config.protocol_fee_bps = 1_000;
    (pool, config)
}

#[test]
fn lbp_quotes_reuse_core_results_for_each_engine_and_trade_form() {
    use super::LbpQuoteRequest::*;
    for weighted in [false, true] {
        let (pool, config) = lbp_pool_snapshot(weighted);
        for request in [
            BuyExactInput {
                collateral_in: 20,
                min_tokens_out: 1,
            },
            BuyExactOutput {
                tokens_out: 5,
                max_collateral_in: 100,
            },
            SellExactInput {
                tokens_in: 10,
                min_collateral_out: 1,
            },
        ] {
            let expected = lbp_core::quote_trade(&pool, request, 50).unwrap();
            let before = pool.clone();
            assert_eq!(
                super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50).unwrap(),
                expected
            );
            assert_eq!(pool, before);
        }
    }
}

#[test]
fn lbp_quotes_reject_each_pool_identity_mismatch_and_wrong_namespace_config() {
    let (pool, config) = lbp_pool_snapshot(false);
    let request = super::LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 0,
    };
    for field in 0..7 {
        let mut wrong = pool.clone();
        let other = AccountId::new([99; 32]);
        match field {
            0 => wrong.namespace = other,
            1 => wrong.namespace_program_id = other,
            2 => wrong.factory_owner = other,
            3 => wrong.factory_program_id = other,
            4 => wrong.factory_seed = [99; 32],
            5 => wrong.token_definition_id = other,
            6 => wrong.collateral_definition_id = other,
            _ => unreachable!(),
        }
        assert!(matches!(
            super::quote_lbp_trade(lbp_sale(), &wrong, &config, request, 50),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
    let mut wrong = config;
    wrong.namespace = AccountId::new([99; 32]);
    assert!(matches!(
        super::quote_lbp_trade(lbp_sale(), &pool, &wrong, request, 50),
        Err(ClientError::PoolSelectionMismatch)
    ));
}

#[test]
fn lbp_quotes_check_live_freeze_and_preserve_core_errors() {
    let request = super::LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 0,
    };
    for weighted in [false, true] {
        let (mut pool, mut config) = lbp_pool_snapshot(weighted);
        config.emergency.is_frozen = true;
        assert!(matches!(
            super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50),
            Err(ClientError::NamespaceFrozen)
        ));
        config.emergency.is_frozen = false;
        pool.trade_mode = super::TradeMode::BuyOnly;
        assert!(matches!(
            super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50),
            Err(ClientError::QuoteLbpTrade(
                lbp_core::Error::TradeDirectionUnsupported
            ))
        ));
        pool.trade_mode = super::TradeMode::BuyAndSell;
        pool.remaining_inventory = 0;
        pool.ending_policy = super::EndingPolicy::Either;
        assert!(matches!(
            super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50),
            Err(ClientError::QuoteLbpTrade(lbp_core::Error::Closed))
        ));
    }
}

#[test]
fn lbp_sell_quotes_apply_net_floor_and_protect_weighted_seed() {
    let (pool, config) = lbp_pool_snapshot(false);
    let request = super::LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 18,
    };
    let quote = super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50).unwrap();
    assert_eq!(
        (
            quote.gross_collateral,
            quote.net_collateral,
            quote.protocol_fee,
            quote.amount_out
        ),
        (20, 18, 2, 18)
    );
    let too_high = super::LbpQuoteRequest::SellExactInput {
        tokens_in: 10,
        min_collateral_out: 19,
    };
    assert!(super::quote_lbp_trade(lbp_sale(), &pool, &config, too_high, 50).is_err());
    let (mut pool, config) = lbp_pool_snapshot(true);
    pool.recorded_collateral = 100;
    if let lbp_core::PoolMode::Weighted {
        pricing_collateral, ..
    } = &mut pool.mode
    {
        *pricing_collateral = 100;
    }
    assert!(super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50).is_err());
}

#[test]
fn lbp_sell_builders_keep_participant_authority_reverse_holdings_and_clock() {
    let sale = lbp_sale();
    let participant = AccountId::new([9; 32]);
    let a = super::lbp_addresses(sale);
    let terms = ExactInputTradeTerms {
        amount_in: 10,
        min_amount_out: 18,
    };
    for weighted in [false, true] {
        let call = if weighted {
            super::build_weighted_sell_invocation(sale, participant, terms)
        } else {
            super::build_fixed_price_sell_invocation(sale, participant, terms, 50)
        };
        assert_eq!(
            call.destination,
            Actor::new(participant, sale.lbp_program_id)
        );
        assert_eq!(call.signer_accounts, vec![participant]);
        let lbp_core::Message::Execute { pool, instruction } = &call.instruction else {
            panic!("expected Execute");
        };
        assert_eq!(*pool, a.pool);
        assert_eq!(
            **instruction,
            if weighted {
                lbp_core::Message::SellWeightedExactInput {
                    amount_in: 10,
                    min_amount_out: 18,
                }
            } else {
                lbp_core::Message::SellExactInput {
                    amount_in: 10,
                    min_amount_out: 18,
                    now: 50,
                }
            }
        );
        assert_eq!(
            call.actors
                .iter()
                .filter(|actor| **actor == clock_core::clock_actor())
                .count(),
            1
        );
        for owner in [participant, a.pool] {
            for definition in [a.token_definition, sale.collateral_definition] {
                assert!(call.actors.contains(&super::token_actor(
                    super::associated_token_account(owner, definition)
                )));
                assert!(call.pda_accounts.contains(&(
                    curve_core::TOKEN_PROGRAM_ACCOUNT_ID,
                    lez_compat_core::token::holding_seed(owner, definition)
                )));
            }
        }
        assert!(
            !call
                .actors
                .iter()
                .any(|a| a.program_account_id == curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID)
        );
    }
}

#[test]
fn retained_sell_fee_is_forwarded_and_immutable_at_each_creation_stage() {
    let mut request = creation();
    request.sell_fee_bps = 250;
    let call =
        build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()).unwrap();
    assert!(matches!(
        call.instruction,
        factory_core::Message::CreateFactoryPool {
            sell_fee_bps: 250,
            ..
        }
    ));
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
        CreationStage::Active,
    ] {
        let mut state = valid_factory_state(sale());
        state.creation_stage = stage;
        state.sell_fee_bps = 250;
        assert!(
            super::next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        for replacement in [0, 251] {
            let mut changed = request.clone();
            changed.sell_fee_bps = replacement;
            assert!(matches!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    changed,
                    Some(&state)
                ),
                Err(ClientError::LaunchParametersMismatch)
            ));
        }
    }
    for rate in [10_000, u16::MAX] {
        request.sell_fee_bps = rate;
        assert!(matches!(
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()),
            Err(ClientError::FactoryPolicy(
                factory_core::Error::InvalidTerms
            ))
        ));
        let mut state = valid_factory_state(sale());
        state.sell_fee_bps = rate;
        assert!(matches!(
            validate_factory_state(sale(), &state),
            Err(ClientError::FactorySaleMismatch)
        ));
    }
}

#[test]
fn factory_pool_binds_initial_k_and_accepts_current_k_after_retained_sell() {
    let mut state = valid_factory_state(sale());
    state.sell_fee_bps = 1_000;
    let mut pool = matching_factory_pool(&state);
    let initial = pool.pool.initial_k;
    let quote = pool
        .pool
        .swap_exact_input(pool::ExactInputSwap {
            token_in: pool::TokenSide::Token0,
            amount_in: 500,
            min_amount_out: 0,
            pool_fee_bps: 0,
            protocol_fee_bps: 500,
            now: 1,
        })
        .unwrap();
    assert!(quote.retained_fee > 0);
    assert!(pool.pool.k > initial);
    assert_eq!(pool.pool.initial_k, initial);
    super::validate_factory_pool(sale(), &state, &pool).unwrap();
    let mut changed = pool.clone();
    changed.pool.initial_k += 1;
    assert!(matches!(
        super::validate_factory_pool(sale(), &state, &changed),
        Err(ClientError::FactorySaleMismatch)
    ));
    changed = pool.clone();
    changed.pool.sell_fee_bps = 999;
    assert!(matches!(
        super::validate_factory_pool(sale(), &state, &changed),
        Err(ClientError::FactorySaleMismatch)
    ));
}

#[test]
fn curve_quotes_expose_retained_fee_without_changing_buy_quotes() {
    let (mut pool, mut config) = pool_snapshot();
    config.protocol_fee_bps = 1_000;
    let zero_sell = quote_sell(&pool, &config, 250, 1).unwrap();
    let zero_buy = quote_buy_with_collateral(&pool, &config, 25, 1).unwrap();
    assert_eq!(zero_sell.retained_fee, 0);
    pool.pool.sell_fee_bps = 500;
    let sell = quote_sell(&pool, &config, 250, 1).unwrap();
    assert_eq!(
        (
            sell.raw_amount_out,
            sell.protocol_fee,
            sell.retained_fee,
            sell.amount_out
        ),
        (20, 2, 1, 17)
    );
    assert_eq!(
        quote_buy_with_collateral(&pool, &config, 25, 1).unwrap(),
        zero_buy
    );
    assert_eq!(pool.pool.k, pool.pool.initial_k);
    config.protocol_fee_bps = 9_500;
    assert!(quote_sell(&pool, &config, 250, 1).is_err());
    assert!(quote_buy_with_collateral(&pool, &config, 100, 1).is_ok());
}

#[test]
fn lbp_quotes_expose_independent_retained_fees_and_net_slippage() {
    for weighted in [false, true] {
        let (mut pool, config) = lbp_pool_snapshot(weighted);
        let request = super::LbpQuoteRequest::SellExactInput {
            tokens_in: 10,
            min_collateral_out: 0,
        };
        let zero = super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50).unwrap();
        assert_eq!(zero.retained_fee, 0);
        let buy = super::LbpQuoteRequest::BuyExactInput {
            collateral_in: 25,
            min_tokens_out: 0,
        };
        let zero_buy = super::quote_lbp_trade(lbp_sale(), &pool, &config, buy, 50).unwrap();
        pool.sell_fee_bps = 500;
        let quote = super::quote_lbp_trade(lbp_sale(), &pool, &config, request, 50).unwrap();
        assert_eq!(quote.gross_collateral, zero.gross_collateral);
        assert_eq!(quote.protocol_fee, zero.protocol_fee);
        assert!(quote.retained_fee > 0);
        assert_eq!(
            quote.amount_out + quote.protocol_fee + quote.retained_fee,
            quote.gross_collateral
        );
        assert_eq!(quote.net_collateral, quote.amount_out);
        assert_eq!(
            super::quote_lbp_trade(lbp_sale(), &pool, &config, buy, 50).unwrap(),
            zero_buy
        );
        let too_high = super::LbpQuoteRequest::SellExactInput {
            tokens_in: 10,
            min_collateral_out: quote.amount_out + 1,
        };
        let core_error = lbp_core::quote_trade(&pool, too_high, 50).unwrap_err();
        let Err(ClientError::QuoteLbpTrade(error)) =
            super::quote_lbp_trade(lbp_sale(), &pool, &config, too_high, 50)
        else {
            panic!("expected core slippage error")
        };
        assert_eq!(error, core_error);
    }
}

#[test]
fn curve_creation_preserves_and_validates_graduation_percentages() {
    for policy in [
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        },
        GraduationPolicy {
            launch_bps: 10000,
            collateral_bps: 10000,
        },
    ] {
        let mut request = creation();
        request.graduation_hook = Some(factory_core::GraduationHook {
            program_id: AccountId::new([19; 32]),
            configuration: vec![],
        });
        request.graduation_policy = policy;
        let call =
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request.clone()).unwrap();
        assert!(
            matches!(call.instruction, factory_core::Message::CreateFactoryPool { graduation_policy, .. } if graduation_policy == policy)
        );
        request.graduation_hook = None;
        assert_eq!(
            build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).is_ok(),
            policy == GraduationPolicy::default()
        );
    }
    for policy in [
        GraduationPolicy {
            launch_bps: 10001,
            collateral_bps: 0,
        },
        GraduationPolicy {
            launch_bps: 0,
            collateral_bps: 10001,
        },
    ] {
        let mut request = creation();
        request.graduation_hook = Some(factory_core::GraduationHook {
            program_id: AccountId::new([19; 32]),
            configuration: vec![],
        });
        request.graduation_policy = policy;
        assert!(build_create_sale_invocation(sale(), AccountId::new([9; 32]), request).is_err());
    }
}

#[test]
fn resumed_curve_creation_cannot_change_the_saved_split() {
    for stage in [
        CreationStage::Minted,
        CreationStage::Allocated,
        CreationStage::PoolPrepared,
        CreationStage::Active,
    ] {
        let mut state = valid_factory_state(sale());
        state.creation_stage = stage;
        state.graduation_hook = Some(factory_core::GraduationHook {
            program_id: AccountId::new([19; 32]),
            configuration: vec![],
        });
        state.graduation_policy = GraduationPolicy {
            launch_bps: 2500,
            collateral_bps: 7500,
        };
        let mut request = creation();
        request.graduation_hook = state.graduation_hook.clone();
        request.graduation_policy = state.graduation_policy;
        assert!(
            super::next_creation_invocation(
                sale(),
                AccountId::new([9; 32]),
                request.clone(),
                Some(&state)
            )
            .is_ok()
        );
        for policy in [
            GraduationPolicy {
                launch_bps: 2501,
                collateral_bps: 7500,
            },
            GraduationPolicy {
                launch_bps: 2500,
                collateral_bps: 7501,
            },
        ] {
            request.graduation_policy = policy;
            assert!(matches!(
                super::next_creation_invocation(
                    sale(),
                    AccountId::new([9; 32]),
                    request.clone(),
                    Some(&state)
                ),
                Err(ClientError::LaunchParametersMismatch)
            ));
        }
    }
}

#[test]
fn curve_graduation_admits_both_current_creator_holdings_without_signing() {
    let mut state = valid_factory_state(sale());
    state.graduation_hook = Some(factory_core::GraduationHook {
        program_id: AccountId::new([19; 32]),
        configuration: vec![],
    });
    for creator_hint in [AccountId::new([21; 32]), AccountId::new([22; 32])] {
        let call = super::build_graduation_invocation(
            super::GraduationTarget::Curve {
                sale: sale(),
                state: &state,
                creator_hint,
            },
            AccountId::new([23; 32]),
        )
        .unwrap();
        assert!(call.signer_accounts.is_empty());
        assert_eq!(
            call.instruction,
            factory_core::Message::ExecuteGraduationHook {
                creator_hint: Some(creator_hint)
            }
        );
        assert!(call.actors.contains(&Actor::new(
            creator_hint,
            curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID
        )));
        for definition in [state.token_definition_id, sale().collateral_definition] {
            let holding = super::associated_token_account(creator_hint, definition);
            assert!(
                call.account_mentions()
                    .iter()
                    .any(|mention| mention.actor() == super::token_actor(holding)
                        && !mention.authorizes
                        && matches!(mention.identity, wallet::AccountIdentity::PublicPda { .. }))
            );
        }
    }
}

#[test]
fn resumed_creation_cannot_change_the_graduation_reserve() {
    let sale = sale();
    let creator = AccountId::new([9; 32]);
    let mut state = valid_factory_state(sale);
    state.creation_stage = CreationStage::Minted;
    state.graduation_reserve = 120;
    state.total_supply += 120;
    let mut request = creation();
    assert!(matches!(
        super::next_creation_invocation(sale, creator, request.clone(), Some(&state)),
        Err(ClientError::LaunchParametersMismatch)
    ));
    request.graduation_reserve = 120;
    let invocation = super::next_creation_invocation(sale, creator, request, Some(&state))
        .unwrap()
        .unwrap();
    assert!(matches!(
        invocation.instruction,
        factory_core::Message::ContinueCreation { .. }
    ));
}
