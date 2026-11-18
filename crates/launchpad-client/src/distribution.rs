//! Transaction construction for the immutable distribution adapter.
use super::*;
pub use distribution_adapter::{
    Configuration, Error as DistributionError, Message, Payout, Rule, State, VERSION,
};

/// Canonical destination declarations, shared by graduation and later distributions.
pub fn admit_destinations<I>(
    mut call: PublicInvocation<I>,
    configuration: &Configuration,
    custody: AccountId,
    definitions: [AccountId; 2],
) -> std::result::Result<PublicInvocation<I>, DistributionError> {
    configuration.validate_custody(custody)?;
    call.actors.push(token_actor(custody));
    for (asset, rules) in [&configuration.launch_rules, &configuration.collateral_rules]
        .into_iter()
        .enumerate()
    {
        let definition = definitions[asset];
        call.actors.push(token_actor(definition));
        call.actors
            .push(token_actor(associated_token_account(custody, definition)));
        call = call.with_holding(custody, definition);
        for rule in rules {
            call.actors.push(token_actor(associated_token_account(
                rule.destination,
                definition,
            )));
            call = call.with_holding(rule.destination, definition);
        }
    }
    let mut seen = std::collections::HashSet::new();
    call.actors.retain(|actor| seen.insert(*actor));
    Ok(call)
}

/// Read configuration only from the saved hook. The caller cannot replace destinations.
pub fn graduation(
    target: GraduationTarget<'_>,
    treasury: AccountId,
) -> Result<PublicInvocation<FactoryInstruction>> {
    let (hook, factory, definitions) = match target {
        GraduationTarget::Curve { sale, state, .. } => (
            state.graduation_hook.as_ref(),
            factory_pool_addresses(sale).factory,
            [state.token_definition_id, state.collateral_definition_id],
        ),
        GraduationTarget::Lbp { sale, state, .. } => (
            state.graduation_hook.as_ref(),
            lbp_addresses(sale).factory,
            [state.token_definition_id, state.collateral_definition_id],
        ),
    };
    let hook = hook.ok_or(ClientError::GraduationNotConfigured)?;
    let configuration =
        Configuration::decode(&hook.configuration).map_err(ClientError::Distribution)?;
    let custody = factory_core::compute_graduation_pda(hook.program_id, factory);
    let call = build_graduation_invocation(target, treasury)?;
    admit_destinations(call, &configuration, custody, definitions)
        .map_err(ClientError::Distribution)
}

pub fn initialize(
    adapter_program: AccountId,
    factory_program: AccountId,
    namespace: AccountId,
    launch_salt: [u8; 32],
    kind: factory_core::GraduationIntentKind,
) -> PublicInvocation<Message> {
    let factory = factory_core::compute_factory_pda(namespace, factory_program, launch_salt);
    let custody = factory_core::compute_graduation_pda(adapter_program, factory);
    invocation(
        Actor::new(custody, adapter_program),
        vec![Actor::new(factory, factory_program)],
        vec![],
        Message::Initialize {
            factory_program_id: factory_program,
            namespace,
            launch_salt,
            accepted_intent: kind,
        },
    )
    .with_pda(
        adapter_program,
        factory_core::compute_graduation_seed(factory),
    )
}

/// The saved state determines all accounts. Admission does not grant caller authority.
pub fn distribute_remaining(
    adapter_program: AccountId,
    state: &State,
) -> std::result::Result<PublicInvocation<Message>, DistributionError> {
    if state.receipt.is_none() {
        return Err(DistributionError::NotGraduated);
    }
    if !state.is_idle() {
        return Err(DistributionError::InvalidState);
    }
    let custody = factory_core::compute_graduation_pda(adapter_program, state.factory.account_id);
    let call = invocation(
        Actor::new(custody, adapter_program),
        vec![],
        vec![],
        Message::DistributeRemaining,
    )
    .with_pda(
        adapter_program,
        factory_core::compute_graduation_seed(state.factory.account_id),
    );
    admit_destinations(call, &state.configuration, custody, state.definitions)
}

pub use factory_core::GraduationIntentKind;

/// Check structure and percentages only. Use `hook` before sale creation.
pub fn encode_configuration(configuration: &Configuration) -> Result<Vec<u8>> {
    configuration
        .validate()
        .map_err(ClientError::Distribution)?;
    borsh::to_vec(configuration).map_err(ClientError::EncodeMessage)
}

/// Build immutable hook terms after checking the exact sale's custody restriction.
/// The returned hook can be used by any of the three sale creation builders.
pub fn hook(
    configuration: &Configuration,
    adapter_program: AccountId,
    factory_program: AccountId,
    namespace: AccountId,
    launch_salt: [u8; 32],
) -> Result<GraduationHook> {
    let factory = factory_core::compute_factory_pda(namespace, factory_program, launch_salt);
    let custody = factory_core::compute_graduation_pda(adapter_program, factory);
    configuration
        .validate_custody(custody)
        .map_err(ClientError::Distribution)?;
    Ok(GraduationHook {
        program_id: adapter_program,
        configuration: encode_configuration(configuration)?,
    })
}

pub async fn load_state(
    wallet: &WalletCore,
    adapter_program: AccountId,
    factory_program: AccountId,
    namespace: AccountId,
    launch_salt: [u8; 32],
) -> Result<State> {
    let factory = factory_core::compute_factory_pda(namespace, factory_program, launch_salt);
    let custody = factory_core::compute_graduation_pda(adapter_program, factory);
    let account = wallet
        .get_account_public(custody)
        .await
        .map_err(|source| ClientError::ReadFactoryState(source.into_boxed_dyn_error()))?
        .ok_or(ClientError::FactoryLaunchMissing)?;
    let state = State::decode(account.data.actor_state(adapter_program).as_ref())
        .map_err(ClientError::Distribution)?;
    if state.factory != Actor::new(factory, factory_program)
        || state.namespace != namespace
        || state.launch_salt != launch_salt
    {
        return Err(ClientError::FactorySaleMismatch);
    }
    state
        .configuration
        .validate_custody(custody)
        .map_err(ClientError::Distribution)?;
    Ok(state)
}
