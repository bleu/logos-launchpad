use lee::AccountId;
use lee_core::account::Actor;

use crate::invocation::{associated_token_account, token_actor};
use crate::lbp::lbp_addresses;
use crate::{
    ExactInputTradeTerms, ExactOutputTradeTerms, build_create_fixed_price_invocation,
    build_create_weighted_invocation, build_fixed_price_buy_invocation,
    build_fixed_price_buy_with_collateral_invocation, build_fixed_price_sell_invocation,
    build_weighted_buy_invocation, build_weighted_buy_with_collateral_invocation,
    build_weighted_sell_invocation,
};

use super::fixtures::{fixed_creation, lbp_sale, weighted_creation};

#[test]
fn fixed_price_exact_input_builder_keeps_membership_and_trader_authority() {
    let selected = lbp_sale();
    let trader = AccountId::new([9; 32]);
    let membership = curve_core::MembershipPath {
        index: 3,
        salt: [7; 32],
        siblings: [[8; 32]; 17],
    };
    let call = build_fixed_price_buy_with_collateral_invocation(
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
    let output = associated_token_account(trader, definition);
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
    let call = build_fixed_price_buy_invocation(
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

#[test]
fn fixed_price_creation_builder_preserves_policy_and_admits_setup_accounts() {
    let selected = lbp_sale();
    let inputs = fixed_creation();
    let call = build_create_fixed_price_invocation(selected.factory_program_id, inputs.clone());
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
fn weighted_exact_input_builder_keeps_membership_limits_and_pricing_clock() {
    let trader = AccountId::new([9; 32]);
    let membership = curve_core::MembershipPath {
        index: 4,
        salt: [5; 32],
        siblings: [[6; 32]; 17],
    };
    let call = build_weighted_buy_with_collateral_invocation(
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
    let call = build_weighted_buy_invocation(
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

#[test]
fn weighted_creation_builder_preserves_policy_weights_and_setup_admission() {
    let inputs = weighted_creation();
    let call = build_create_weighted_invocation(lbp_sale().factory_program_id, inputs.clone());
    assert_eq!(
        call.instruction,
        factory_core::Message::CreateWeightedLbp { inputs }
    );
    let fixed =
        build_create_fixed_price_invocation(lbp_sale().factory_program_id, fixed_creation());
    assert_eq!(call.destination, fixed.destination);
    assert_eq!(call.signer_accounts, fixed.signer_accounts);
    assert_eq!(call.actors, fixed.actors);
    assert_eq!(call.pda_accounts, fixed.pda_accounts);
}

#[test]
fn lbp_sell_builders_keep_participant_authority_reverse_holdings_and_clock() {
    let sale = lbp_sale();
    let participant = AccountId::new([9; 32]);
    let a = lbp_addresses(sale);
    let terms = ExactInputTradeTerms {
        amount_in: 10,
        min_amount_out: 18,
    };
    for weighted in [false, true] {
        let call = if weighted {
            build_weighted_sell_invocation(sale, participant, terms)
        } else {
            build_fixed_price_sell_invocation(sale, participant, terms, 50)
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
                assert!(
                    call.actors
                        .contains(&token_actor(associated_token_account(owner, definition)))
                );
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
