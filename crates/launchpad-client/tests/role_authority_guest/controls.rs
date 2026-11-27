use crate::fixture::{
    COLLATERAL, CURVE, FactoryFixture, SALT, TOKEN, TREASURY, balance, execute, factory_fixture,
    holding, id, key,
};
use lee::{AccountId, Actor, PrivateKey, V03State};
use lez_compat_core::token;

#[path = "controls/authority.rs"]
mod authority;
#[path = "controls/trading.rs"]
mod trading;

fn namespace_config(state: &V03State, namespace: AccountId) -> curve_core::Config {
    curve_core::Config::decode(
        state
            .get_account_by_id(curve_core::compute_config_pda(namespace, CURVE))
            .data
            .actor_state(CURVE),
    )
    .unwrap()
}

fn admin_call(
    state: &mut V03State,
    actors: &[Actor],
    signer: &PrivateKey,
    message: curve_core::Message,
) -> Result<(), lee::error::LeeError> {
    execute(
        state,
        Actor::new(id(signer), CURVE),
        actors.to_vec(),
        message,
        &[signer],
    )
}

fn emergency_key() -> PrivateKey {
    let emergency = key(52);
    assert_eq!(
        id(&emergency).value(),
        curve_core::INITIAL_EMERGENCY_AUTHORITY.value()
    );
    emergency
}

fn fresh_sale(trade_mode: u8, end_timestamp: Option<u64>) -> FactoryFixture {
    sale_with_pause(trade_mode, end_timestamp, curve_core::PausePolicy::Disabled)
}

fn sale_with_pause(
    trade_mode: u8,
    end_timestamp: Option<u64>,
    pause_policy: curve_core::PausePolicy,
) -> FactoryFixture {
    let mut fixture = factory_fixture();
    let creator = key(48);
    fixture.actors.extend([
        Actor::new(TREASURY, TOKEN),
        holding(TREASURY, COLLATERAL),
        Actor::new(id(&key(52)), CURVE),
    ]);
    fixture
        .call(
            &creator,
            factory_core::Message::CreateFactoryPool {
                buy_policy: curve_core::BuyPolicy::Open,
                pause_policy,
                ending_policy: if end_timestamp.is_some() {
                    curve_core::EndingPolicy::Either
                } else {
                    curve_core::EndingPolicy::Depletion
                },
                trade_mode,
                sell_fee_bps: 0,
                per_block_token_limit: None,
                graduation_hook_program_id: None,
                graduation_hook_configuration: vec![],
                namespace: fixture.namespace,
                launch_salt: SALT,
                name: "Controls sale".into(),
                uri: "ipfs://controls".into(),
                graduation_reserve: 0,
                sale_reserve: 400,
                graduation_policy: launchpad_client::GraduationPolicy::default(),
                creator_allocation: 50,
                virtual_token_reserve: 1_000,
                virtual_collateral_reserve: 1_000,
                end_timestamp,
                curve_program_id: CURVE,
                collateral_definition_id: COLLATERAL,
                creator: id(&creator),
                now: 1,
            },
        )
        .unwrap();
    assert!(saved_pool(&fixture).funded);
    assert_eq!(
        balance(&fixture.state, fixture.pool, fixture.definition),
        400
    );
    execute(
        &mut fixture.state,
        Actor::new(fixture.definition, TOKEN),
        fixture.actors.clone(),
        token::Message::Initialize {
            owner: id(&key(49)),
        },
        &[],
    )
    .unwrap();
    execute(
        &mut fixture.state,
        Actor::new(COLLATERAL, TOKEN),
        fixture.actors.clone(),
        token::Message::Initialize { owner: TREASURY },
        &[],
    )
    .unwrap();
    fixture
}

fn saved_pool(fixture: &FactoryFixture) -> curve_core::PoolAccount {
    curve_core::PoolAccount::decode(
        fixture
            .state
            .get_account_by_id(fixture.pool)
            .data
            .actor_state(CURVE),
    )
    .unwrap()
}
