//! Run after building methods, with RISC0_DEV_MODE=1. Executes actual pinned guest ELFs.
use lee::{
    error::LeeError,
    privacy_preserving_transaction::circuit::{ProgramWithDependencies, execute_and_prove},
    program::Program,
};
use lee_core::{
    EncryptionScheme, EphemeralPublicKey, InputAccountIdentity, SharedSecretKey,
    account::{Account, AccountId, AccountWithMetadata, Data, Nonce},
};
use std::{collections::HashMap, path::PathBuf};
use token_core::TokenHolding;

fn guest(name: &str) -> Program {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../methods/target/riscv-guest/launchpad_methods/launchpad_programs/riscv32im-risc0-zkvm-elf/release");
    launchpad_client::load_program(&root.join(format!("{name}.bin")))
        .expect("build methods before running guest tests")
}

fn ata(owner: AccountId, definition: AccountId) -> AccountId {
    associated_token_account_core::get_associated_token_account_id(
        &programs::ata().id(),
        &associated_token_account_core::compute_ata_seed(owner, definition),
    )
}

#[test]
fn private_nft_closes_a_direct_pool() {
    let router = guest("private_authority");
    let curve = guest("curve");
    let namespace = AccountId::new([81; 32]);
    let nft = AccountId::new([85; 32]);
    let token0 = AccountId::new([86; 32]);
    let token1 = AccountId::new([87; 32]);
    let pool_id = curve_core::compute_pool_pda(namespace, curve.id(), token0, token1, nft);
    let pool = curve_core::PoolAccount {
        allowed_input_token: None,
        funded: true,
        namespace,
        token0_definition_id: token0,
        token1_definition_id: token1,
        owner: nft,
        owner_program: None,
        pool: pool::Pool::create(pool::PoolCreateParams {
            token0_amount: 800,
            token1_amount: 0,
            virtual_reserve0: 2_000,
            virtual_reserve1: 100,
            close_timestamp: None,
            close_on_depletion: Some(pool::TokenSide::Token0),
        })
        .unwrap(),
    };
    let pool_account = AccountWithMetadata::new(
        Account {
            program_owner: curve.id(),
            data: Data::from(&pool),
            ..Account::default()
        },
        false,
        pool_id,
    );
    let clock = AccountWithMetadata::new(
        Account {
            program_owner: [88; 8],
            data: Data::try_from(
                clock_core::ClockAccountData {
                    block_id: 1,
                    timestamp: 1,
                }
                .to_bytes(),
            )
            .unwrap(),
            ..Account::default()
        },
        false,
        clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID,
    );
    let transient = AccountWithMetadata::new(Account::default(), true, AccountId::new([82; 32]));
    let nsk = [4; 32];
    let source_id =
        AccountId::for_regular_private_account(&lee_core::NullifierPublicKey::from(&nsk), 0);
    let source = AccountWithMetadata::new(
        Account {
            program_owner: programs::token().id(),
            data: Data::from(&TokenHolding::NftMaster {
                definition_id: nft,
                print_balance: 1,
            }),
            nonce: Nonce::private_account_nonce_init(&source_id),
            ..Account::default()
        },
        true,
        source_id,
    );
    let ssk = SharedSecretKey([5; 32]);
    let identities = vec![
        InputAccountIdentity::PrivateAuthorizedUpdate {
            epk: EphemeralPublicKey(Vec::new()),
            view_tag: Default::default(),
            ssk,
            nsk,
            membership_proof: (0, vec![]),
            identifier: 0,
        },
        InputAccountIdentity::Public,
        InputAccountIdentity::Public,
        InputAccountIdentity::Public,
    ];
    let graph = ProgramWithDependencies::new(
        router,
        HashMap::from([
            (curve.id(), curve.clone()),
            (programs::token().id(), programs::token()),
        ]),
    );
    let instruction = private_flow_core::PrivateAuthorityInstruction {
        program_id: curve.id(),
        instruction_data: Program::serialize_instruction(curve_core::Instruction::ClosePool)
            .unwrap(),
        authority_index: 1,
    };
    let pre = vec![source, pool_account, transient, clock];
    let (output, _) = execute_and_prove(
        pre.clone(),
        Program::serialize_instruction(instruction.clone()).unwrap(),
        identities.clone(),
        &graph,
    )
    .expect("private direct close");
    let (_, returned) = EncryptionScheme::decrypt(
        &output.encrypted_private_post_states[0].ciphertext,
        &ssk,
        &output.new_commitments[0],
        0,
    )
    .unwrap();
    assert_eq!(
        TokenHolding::try_from(&returned.data).unwrap(),
        TokenHolding::NftMaster {
            definition_id: nft,
            print_balance: 1
        }
    );
    assert!(output.public_post_states.iter().any(|post| {
        curve_core::PoolAccount::try_from(&post.data)
            .is_ok_and(|state| state.pool.effective_lifecycle(1) == pool::PoolLifecycle::Closed)
    }));
    assert_eq!(output.new_nullifiers.len(), 1);
    assert_eq!(output.new_commitments.len(), 1);
    let mut bad_pre = pre;
    bad_pre[0].account.data = Data::from(&TokenHolding::NftMaster {
        definition_id: AccountId::new([84; 32]),
        print_balance: 1,
    });
    assert!(
        execute_and_prove(
            bad_pre,
            Program::serialize_instruction(instruction).unwrap(),
            identities,
            &graph
        )
        .is_err()
    );
}

#[test]
fn private_buy_execute_envelope_reaches_the_platform_call_limit() {
    let router = guest("private_buy");
    let curve = guest("curve");
    let namespace = AccountId::new([91; 32]);
    let owner = AccountId::new([92; 32]);
    let collateral = AccountId::new([93; 32]);
    let launch = AccountId::new([94; 32]);
    let pool_id = curve_core::compute_pool_pda(namespace, curve.id(), launch, collateral, owner);
    let config_id = curve_core::compute_config_pda(namespace, curve.id());
    let transient_id = AccountId::new([95; 32]);
    let destination_nsk = [96; 32];
    let destination_npk = lee_core::NullifierPublicKey::from(&destination_nsk);
    let destination_id = AccountId::for_regular_private_account(&destination_npk, 0);
    let nsk = [97; 32];
    let source_id =
        AccountId::for_regular_private_account(&lee_core::NullifierPublicKey::from(&nsk), 0);
    let source = AccountWithMetadata::new(
        Account {
            balance: 10,
            program_owner: programs::token().id(),
            data: Data::from(&TokenHolding::Fungible {
                definition_id: collateral,
                balance: 100,
            }),
            nonce: Nonce::private_account_nonce_init(&source_id),
        },
        true,
        source_id,
    );
    let definition = |name: &str| Account {
        program_owner: programs::token().id(),
        data: Data::from(&token_core::TokenDefinition::Fungible {
            name: name.into(),
            total_supply: 1_000,
            metadata_id: None,
        }),
        ..Account::default()
    };
    let pool = curve_core::PoolAccount {
        allowed_input_token: None,
        funded: true,
        namespace,
        token0_definition_id: launch,
        token1_definition_id: collateral,
        owner,
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
    let holding = |definition_id, balance| Account {
        program_owner: programs::token().id(),
        data: Data::from(&TokenHolding::Fungible {
            definition_id,
            balance,
        }),
        ..Account::default()
    };
    let clock = Account {
        program_owner: [88; 8],
        data: Data::try_from(
            clock_core::ClockAccountData {
                block_id: 1,
                timestamp: 1,
            }
            .to_bytes(),
        )
        .unwrap(),
        ..Account::default()
    };
    let pre = vec![
        source,
        AccountWithMetadata::new(Account::default(), true, transient_id),
        AccountWithMetadata::new(definition("Collateral"), false, collateral),
        AccountWithMetadata::new(definition("Launch"), false, launch),
        AccountWithMetadata::new(Account::default(), false, ata(transient_id, collateral)),
        AccountWithMetadata::new(Account::default(), false, ata(transient_id, launch)),
        AccountWithMetadata::new(
            Account {
                program_owner: curve.id(),
                data: Data::from(&pool),
                ..Account::default()
            },
            false,
            pool_id,
        ),
        AccountWithMetadata::new(
            Account {
                program_owner: curve.id(),
                data: Data::from(&curve_core::Config::new(
                    namespace,
                    0,
                    AccountId::new([98; 32]),
                    namespace,
                )),
                ..Account::default()
            },
            false,
            config_id,
        ),
        AccountWithMetadata::new(holding(collateral, 100), false, ata(pool_id, collateral)),
        AccountWithMetadata::new(holding(launch, 800), false, ata(pool_id, launch)),
        AccountWithMetadata::new(clock, false, clock_core::CLOCK_01_PROGRAM_ACCOUNT_ID),
        AccountWithMetadata::new(Account::default(), false, destination_id),
    ];
    let instruction = private_flow_core::PrivateBuyInstruction::Execute {
        curve_program_id: curve.id(),
        token_program_id: programs::token().id(),
        ata_program_id: programs::ata().id(),
        native_transfer_program_id: programs::authenticated_transfer().id(),
        amount_out: 25,
        max_collateral_in: 100,
        gas_reserve: 1,
        collateral_definition_id: collateral,
    };
    let graph = ProgramWithDependencies::new(
        router,
        HashMap::from([
            (curve.id(), curve),
            (programs::token().id(), programs::token()),
            (programs::ata().id(), programs::ata()),
            (
                programs::authenticated_transfer().id(),
                programs::authenticated_transfer(),
            ),
        ]),
    );
    let source_ssk = SharedSecretKey([99; 32]);
    let destination_ssk = SharedSecretKey([100; 32]);
    let identities = std::iter::once(InputAccountIdentity::PrivateAuthorizedUpdate {
        epk: EphemeralPublicKey(Vec::new()),
        view_tag: Default::default(),
        ssk: source_ssk,
        nsk,
        membership_proof: (0, vec![]),
        identifier: 0,
    })
    .chain(std::iter::repeat_n(InputAccountIdentity::Public, 10))
    .chain(std::iter::once(InputAccountIdentity::PrivateUnauthorized {
        epk: EphemeralPublicKey(Vec::new()),
        view_tag: Default::default(),
        npk: destination_npk,
        ssk: destination_ssk,
        identifier: 0,
    }))
    .collect();
    // The guest accepts the envelope, but the full route exceeds the pinned call budget.
    assert!(matches!(
        execute_and_prove(
            pre,
            Program::serialize_instruction(instruction).unwrap(),
            identities,
            &graph,
        ),
        Err(LeeError::MaxChainedCallsDepthExceeded)
    ));
}
