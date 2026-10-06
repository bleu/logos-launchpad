use event_probe::compatibility::{Instruction, State};
use lee::{
    Account, AccountId, PrivateKey, PublicKey, PublicTransaction, V03State, program::Program,
    public_transaction,
};
use lee_core::account::ProgramShardSelector;
use token_core::{MetadataStandard, NewTokenDefinition, NewTokenMetadata, token_account_id};

struct Fixture {
    state: V03State,
    program: AccountId,
    config: AccountId,
    definition: AccountId,
    holder: PrivateKey,
}

impl Fixture {
    fn new() -> Self {
        let guest = Program::new(event_probe_methods::COMPATIBILITY_ELF.into()).unwrap();
        let program = AccountId::from_builtin_program(guest.id());
        let config = AccountId::new([12; 32]);
        let definition = AccountId::new([13; 32]);
        let holder = PrivateKey::try_new([1; 32]).unwrap();
        let initial = State {
            authority_definition: definition,
            authorizations: 0,
        };
        let state = V03State::new()
            .with_programs([guest])
            .with_named_programs([(token_account_id(), programs::token())])
            .with_public_accounts([(
                config,
                Account::default().with_shard(
                    program,
                    borsh::to_vec(&initial).unwrap().try_into().unwrap(),
                ),
            )]);
        let mut fixture = Self {
            state,
            program,
            config,
            definition,
            holder,
        };
        let mint = public_transaction::Message::try_new(
            token_account_id(),
            [
                definition,
                account_id(&fixture.holder),
                AccountId::new([14; 32]),
            ]
            .map(|id| ProgramShardSelector::new(id, token_account_id()))
            .to_vec(),
            vec![],
            token_core::Instruction::NewDefinitionWithMetadata {
                new_definition: NewTokenDefinition::NonFungible {
                    name: "Authority".into(),
                    printable_supply: 1,
                },
                metadata: Box::new(NewTokenMetadata {
                    standard: MetadataStandard::Simple,
                    uri: String::new(),
                    creators: String::new(),
                }),
            },
        )
        .unwrap();
        let witnesses = public_transaction::WitnessSet::for_message(&mint, &[]);
        fixture
            .state
            .transition_from_public_transaction(&PublicTransaction::new(mint, witnesses), 1, 0)
            .unwrap();
        fixture
    }

    fn authorize(&self, key: &PrivateKey, definition: AccountId) -> PublicTransaction {
        let holder = account_id(key);
        let message = public_transaction::Message::try_new(
            self.program,
            vec![
                ProgramShardSelector::new(self.config, self.program),
                ProgramShardSelector::new(holder, token_account_id()),
            ],
            vec![self.state.get_account_by_id(holder).nonce],
            Instruction::Authorize { definition },
        )
        .unwrap();
        let witnesses = public_transaction::WitnessSet::for_message(&message, &[key]);
        PublicTransaction::new(message, witnesses)
    }

    fn transfer_authority(&mut self, recipient: AccountId) {
        let holder = account_id(&self.holder);
        let message = public_transaction::Message::try_new(
            token_account_id(),
            vec![
                ProgramShardSelector::new(holder, token_account_id()),
                ProgramShardSelector::new(recipient, token_account_id()),
            ],
            vec![self.state.get_account_by_id(holder).nonce],
            token_core::Instruction::Transfer {
                amount_to_transfer: 1,
                descriptor: token_core::TokenDescriptor {
                    definition_id: self.definition,
                    kind: token_core::TokenKind::NftMaster,
                },
            },
        )
        .unwrap();
        let witnesses = public_transaction::WitnessSet::for_message(&message, &[&self.holder]);
        self.state
            .transition_from_public_transaction(&PublicTransaction::new(message, witnesses), 2, 0)
            .unwrap();
    }

    fn reject(&mut self, transaction: &PublicTransaction) {
        let before = borsh::to_vec(&self.state).unwrap();
        let error = self
            .state
            .transition_from_public_transaction(transaction, 3, 0)
            .unwrap_err();
        assert!(
            matches!(
                error,
                lee::error::LeeError::ProgramExecutionFailed(_)
                    | lee::error::LeeError::ProgramExitedWithCode { .. }
            ),
            "unexpected rejection: {error:?}"
        );
        assert_eq!(borsh::to_vec(&self.state).unwrap(), before);
    }

    fn config(&self) -> State {
        borsh::from_slice(
            self.state
                .get_account_by_id(self.config)
                .data
                .shard(self.program)
                .as_ref(),
        )
        .unwrap()
    }
}

fn account_id(key: &PrivateKey) -> AccountId {
    AccountId::from(&PublicKey::new_from_private_key(key))
}

#[test]
fn signed_authority_nft_holder_can_update_its_config() {
    let mut fixture = Fixture::new();
    let transaction = fixture.authorize(&fixture.holder, fixture.definition);
    fixture
        .state
        .transition_from_public_transaction(&transaction, 2, 0)
        .unwrap();
    assert_eq!(fixture.config().authorizations, 1);
}

#[test]
fn authority_follows_the_transferred_master_and_rejects_the_former_holder() {
    let mut fixture = Fixture::new();
    let successor = PrivateKey::try_new([2; 32]).unwrap();
    fixture.transfer_authority(account_id(&successor));
    let former = fixture.authorize(&fixture.holder, fixture.definition);
    fixture.reject(&former);
    let current = fixture.authorize(&successor, fixture.definition);
    fixture
        .state
        .transition_from_public_transaction(&current, 4, 0)
        .unwrap();
    assert_eq!(fixture.config().authorizations, 1);
}
