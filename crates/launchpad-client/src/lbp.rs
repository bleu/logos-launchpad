use curve_core::compute_config_pda;
use factory_core::{
    Message as FactoryInstruction, compute_definition_pda, compute_escrow_pda, compute_factory_pda,
};
use lee::AccountId;
use lee_core::account::Actor;

use crate::invocation::{
    FactoryPoolAddresses, associated_token_account, clock_actor, invocation, token_actor,
};
use crate::{
    ClientError, ExactInputTradeTerms, ExactOutputTradeTerms, LbpSale, MembershipPath,
    PublicInvocation, Result,
};

pub(super) fn validate_lbp_factory_state(
    sale: LbpSale,
    state: &factory_core::LbpLaunchState,
) -> Result<()> {
    let a = lbp_addresses(sale);
    if state.namespace != sale.namespace
        || state.launch_salt != sale.launch_salt
        || state.namespace_program_id != sale.namespace_program_id
        || state.lbp_program_id != sale.lbp_program_id
        || state.collateral_definition_id != sale.collateral_definition
        || state.token_definition_id != a.token_definition
        || state.creator_escrow_id != a.creator_escrow
        || state.pool_id != a.pool
        || state.creator_role_id
            != factory_core::compute_creator_role_pda(
                sale.namespace,
                sale.factory_program_id,
                sale.launch_salt,
            )
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    Ok(())
}

pub(super) fn lbp_addresses(sale: LbpSale) -> FactoryPoolAddresses {
    let factory = compute_factory_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let token_definition =
        compute_definition_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    FactoryPoolAddresses {
        factory,
        token_definition,
        creator_escrow: compute_escrow_pda(
            sale.namespace,
            sale.factory_program_id,
            sale.launch_salt,
        ),
        pool: lbp_core::compute_pool_pda(
            sale.namespace,
            sale.lbp_program_id,
            token_definition,
            sale.collateral_definition,
            factory,
        ),
    }
}

pub(super) fn lbp_setup_invocation(
    sale: LbpSale,
    creator: AccountId,
    instruction: FactoryInstruction,
) -> PublicInvocation<FactoryInstruction> {
    let a = lbp_addresses(sale);
    let mint =
        factory_core::compute_mint_pda(sale.namespace, sale.factory_program_id, sale.launch_salt);
    let role = factory_core::compute_creator_role_pda(
        sale.namespace,
        sale.factory_program_id,
        sale.launch_salt,
    );
    let actors = vec![
        Actor::new(a.factory, sale.factory_program_id),
        Actor::new(creator, sale.factory_program_id),
        Actor::new(role, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(creator, curve_core::AUTHORITY_PROGRAM_ACCOUNT_ID),
        Actor::new(a.pool, sale.lbp_program_id),
        Actor::new(a.factory, sale.lbp_program_id),
        Actor::new(
            compute_config_pda(sale.namespace, sale.namespace_program_id),
            sale.namespace_program_id,
        ),
        token_actor(mint),
        token_actor(a.token_definition),
        token_actor(sale.collateral_definition),
        token_actor(a.factory),
        token_actor(a.pool),
        token_actor(a.creator_escrow),
        token_actor(creator),
        clock_actor(),
    ];
    let mut call = invocation(
        Actor::new(a.factory, sale.factory_program_id),
        actors,
        vec![creator],
        instruction,
    );
    for seed in [
        factory_core::compute_factory_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_creator_role_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_definition_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_mint_seed(sale.namespace, sale.launch_salt),
        factory_core::compute_escrow_seed(sale.namespace, sale.launch_salt),
    ] {
        call = call.with_pda(sale.factory_program_id, seed);
    }
    call = call.with_pda(
        sale.lbp_program_id,
        lbp_core::compute_pool_pda_seed(
            sale.namespace,
            a.token_definition,
            sale.collateral_definition,
            a.factory,
        ),
    );
    for owner in [mint, a.factory, a.pool, a.creator_escrow, creator] {
        for definition in [a.token_definition, sale.collateral_definition] {
            call.actors
                .push(token_actor(associated_token_account(owner, definition)));
            call = call.with_holding(owner, definition);
        }
    }
    call
}

pub fn build_set_lbp_paused_invocation(
    sale: LbpSale,
    creator: AccountId,
    paused: bool,
) -> PublicInvocation<FactoryInstruction> {
    lbp_setup_invocation(
        sale,
        creator,
        FactoryInstruction::SetPaused { creator, paused },
    )
}

pub fn build_create_fixed_price_invocation(
    factory_program_id: AccountId,
    inputs: factory_core::FixedPriceInputs,
) -> PublicInvocation<FactoryInstruction> {
    let sale = LbpSale {
        namespace: inputs.namespace,
        namespace_program_id: inputs.namespace_program_id,
        factory_program_id,
        lbp_program_id: inputs.lbp_program_id,
        launch_salt: inputs.launch_salt,
        collateral_definition: inputs.collateral_definition_id,
    };
    lbp_setup_invocation(
        sale,
        inputs.creator,
        FactoryInstruction::CreateFixedPriceLbp { inputs },
    )
}

pub fn build_create_weighted_invocation(
    factory_program_id: AccountId,
    inputs: factory_core::WeightedInputs,
) -> PublicInvocation<FactoryInstruction> {
    let sale = LbpSale {
        namespace: inputs.namespace,
        namespace_program_id: inputs.namespace_program_id,
        factory_program_id,
        lbp_program_id: inputs.lbp_program_id,
        launch_salt: inputs.launch_salt,
        collateral_definition: inputs.collateral_definition_id,
    };
    lbp_setup_invocation(
        sale,
        inputs.creator,
        FactoryInstruction::CreateWeightedLbp { inputs },
    )
}

fn lbp_trade_invocation(
    sale: LbpSale,
    participant: AccountId,
    instruction: lbp_core::Message,
) -> PublicInvocation<lbp_core::Message> {
    let a = lbp_addresses(sale);
    let mut actors = vec![
        Actor::new(a.pool, sale.lbp_program_id),
        Actor::new(
            compute_config_pda(sale.namespace, sale.namespace_program_id),
            sale.namespace_program_id,
        ),
        token_actor(participant),
        token_actor(a.pool),
        clock_actor(),
    ];
    for owner in [participant, a.pool] {
        for definition in [a.token_definition, sale.collateral_definition] {
            actors.push(token_actor(associated_token_account(owner, definition)));
        }
    }
    let mut call = invocation(
        Actor::new(participant, sale.lbp_program_id),
        actors,
        vec![participant],
        lbp_core::Message::Execute {
            pool: a.pool,
            instruction: Box::new(instruction),
        },
    );
    for owner in [participant, a.pool] {
        for definition in [a.token_definition, sale.collateral_definition] {
            call = call.with_holding(owner, definition);
        }
    }
    call
}

pub fn build_weighted_buy_with_collateral_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyWeightedExactInput {
            membership,
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
        },
    )
}

pub fn build_weighted_buy_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyWeightedExactOutput {
            membership,
            amount_out: terms.amount_out,
            max_amount_in: terms.max_amount_in,
        },
    )
}

pub fn build_fixed_price_buy_with_collateral_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyExactInput {
            membership,
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            now,
        },
    )
}

pub fn build_fixed_price_buy_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactOutputTradeTerms,
    membership: Option<MembershipPath>,
    now: u64,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::BuyExactOutput {
            membership,
            amount_out: terms.amount_out,
            max_amount_in: terms.max_amount_in,
            now,
        },
    )
}

pub fn build_fixed_price_sell_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
    now: u64,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::SellExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
            now,
        },
    )
}

pub fn build_weighted_sell_invocation(
    sale: LbpSale,
    participant: AccountId,
    terms: ExactInputTradeTerms,
) -> PublicInvocation<lbp_core::Message> {
    lbp_trade_invocation(
        sale,
        participant,
        lbp_core::Message::SellWeightedExactInput {
            amount_in: terms.amount_in,
            min_amount_out: terms.min_amount_out,
        },
    )
}
