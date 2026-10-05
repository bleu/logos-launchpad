use eligibility_core::{
    AccountControl, AllowanceBook, ApprovedAccount, Digest, EligibilityError, EligibilityPermit,
    EligibilityWitness, Membership, MembershipTree, Purchase, PurchaseError, Route, SaleContext,
    SalePolicy, TradeOutcome, TreeHash, check_eligibility, check_public_binding,
};

fn digest(hex: &str) -> Digest {
    hex::decode(hex).unwrap().try_into().unwrap()
}

fn public_purchase() -> (SalePolicy, Purchase, EligibilityWitness) {
    let policy = SalePolicy {
        sale: SaleContext {
            deployment: [1; 32],
            factory: [2; 32],
            namespace: [3; 32],
            sale_account: [4; 32],
        },
        root: digest("628482ad6a54642544741dded7aa8f0883fa9d7cb2a57301327178a49725af26"),
        hash: TreeHash::Sha256,
        depth: 0,
        collateral_limit: 100,
    };
    let purchase = Purchase {
        trading_account: [6; 32],
        recipient: [7; 32],
        collateral_in: 10,
        tokens_out: 20,
    };
    let witness = EligibilityWitness {
        control: AccountControl::Public {
            key: digest("0000000000000000000000000000000000000000000000000000000000000003"),
        },
        membership: Membership {
            blinder: [5; 32],
            index: 0,
            siblings: vec![],
        },
    };
    (policy, purchase, witness)
}

// BIP340 vector 0 supplies the key; Python hashlib supplies the encoded digests.
#[test]
fn approved_public_account_authorizes_the_bound_purchase() {
    let (policy, purchase, witness) = public_purchase();
    let expected = EligibilityPermit {
        sale: digest("deacaebd85d7044674c135bf0e9f43c63f1a22f165e9fd5c6fe715437e9dff5f"),
        root: policy.root,
        hash: TreeHash::Sha256,
        collateral_limit: 100,
        purchase: digest("3ef95bafc82fe12d758548697a12dfa79438939dc32ccadcaf44e0eea7278108"),
        nullifier: digest("f8bcfd4fa8fefb511c2708862bffc01f0f2267af81c272ef630becd72770db13"),
    };
    assert_eq!(
        check_eligibility(&policy, &purchase, &witness),
        Ok(expected)
    );
}

#[test]
fn approved_account_at_a_right_hand_leaf_is_accepted() {
    let (mut policy, purchase, mut witness) = public_purchase();
    policy.depth = 1;
    policy.root = digest("702862fa3fa29d31440bc88e56fb36d537ff131c35adeef210bb6a7fbd3941aa");
    witness.membership.index = 1;
    witness.membership.siblings = vec![[8; 32]];
    assert!(check_eligibility(&policy, &purchase, &witness).is_ok());
}

#[test]
fn malformed_membership_paths_are_rejected() {
    for (depth, index, siblings) in [(0, 1, vec![]), (1, 0, vec![]), (18, 0, vec![])] {
        let (mut policy, purchase, mut witness) = public_purchase();
        policy.depth = depth;
        witness.membership.index = index;
        witness.membership.siblings = siblings;
        assert_eq!(
            check_eligibility(&policy, &purchase, &witness),
            Err(EligibilityError::InvalidMembership)
        );
    }
}

#[test]
fn a_purchase_above_the_common_collateral_cap_is_rejected() {
    let (policy, mut purchase, witness) = public_purchase();
    purchase.collateral_in = 101;
    assert!(check_eligibility(&policy, &purchase, &witness).is_err());
}

#[test]
fn zero_value_purchases_do_not_authorize_allowance_use() {
    for (collateral_in, tokens_out) in [(0, 20), (10, 0)] {
        let (policy, mut purchase, witness) = public_purchase();
        purchase.collateral_in = collateral_in;
        purchase.tokens_out = tokens_out;
        assert_eq!(
            check_eligibility(&policy, &purchase, &witness),
            Err(EligibilityError::InvalidPurchase)
        );
    }
}

#[test]
fn creator_prepared_tree_delivers_a_valid_path_without_buyer_setup() {
    let (mut policy, purchase, mut witness) = public_purchase();
    let tree = MembershipTree::prepare(
        &policy.sale,
        TreeHash::Sha256,
        2,
        vec![
            ApprovedAccount {
                account: digest("ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8"),
                blinder: [5; 32],
            },
            ApprovedAccount {
                account: digest("a5342820e7ab710a41f19c489acf7ac00f2e32fd69a4595428bfb67740ff438e"),
                blinder: [5; 32],
            },
        ],
    )
    .unwrap();
    assert_eq!(
        tree.root(),
        digest("cd5738cc958eeee8567eb265083e16ab0734279231aff2541ffb5fafa24caf88")
    );
    policy.root = tree.root();
    policy.depth = 2;
    witness.membership = tree.membership(0).unwrap();
    assert!(check_eligibility(&policy, &purchase, &witness).is_ok());
    assert!(tree.membership(2).is_err());
}

#[test]
fn successful_purchase_spends_the_allowance_across_routes_and_trade_accounts() {
    let (policy, mut purchase, witness) = public_purchase();
    let mut book = AllowanceBook::default();
    assert!(
        book.attempt_purchase(
            &policy,
            &purchase,
            &witness,
            Route::Public,
            TradeOutcome::Succeeded
        )
        .is_ok()
    );
    purchase.trading_account = [99; 32];
    purchase.recipient = [98; 32];
    assert_eq!(
        book.attempt_purchase(
            &policy,
            &purchase,
            &witness,
            Route::Private,
            TradeOutcome::Succeeded
        ),
        Err(PurchaseError::AlreadyUsed)
    );
}

#[test]
fn a_rejected_trade_leaves_the_allowance_for_a_later_purchase() {
    let (policy, purchase, witness) = public_purchase();
    let mut book = AllowanceBook::default();
    assert_eq!(
        book.attempt_purchase(
            &policy,
            &purchase,
            &witness,
            Route::Private,
            TradeOutcome::Rejected
        ),
        Err(PurchaseError::TradeRejected)
    );
    assert!(
        book.attempt_purchase(
            &policy,
            &purchase,
            &witness,
            Route::Public,
            TradeOutcome::Succeeded
        )
        .is_ok()
    );
}

#[test]
fn duplicate_credentials_and_equivalent_bip340_keys_share_one_allowance() {
    let (mut policy, purchase, mut witness) = public_purchase();
    let account = digest("ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8");
    let tree = MembershipTree::prepare(
        &policy.sale,
        TreeHash::Sha256,
        1,
        vec![
            ApprovedAccount {
                account,
                blinder: [5; 32],
            },
            ApprovedAccount {
                account,
                blinder: [9; 32],
            },
        ],
    )
    .unwrap();
    policy.root = tree.root();
    policy.depth = 1;
    witness.membership = tree.membership(0).unwrap();
    let mut book = AllowanceBook::default();
    let permit = book
        .attempt_purchase(
            &policy,
            &purchase,
            &witness,
            Route::Public,
            TradeOutcome::Succeeded,
        )
        .unwrap();
    assert_eq!(
        permit.nullifier,
        digest("f8bcfd4fa8fefb511c2708862bffc01f0f2267af81c272ef630becd72770db13")
    );
    witness.membership = tree.membership(1).unwrap();
    witness.control = AccountControl::Public {
        key: digest("fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd036413e"),
    };
    assert_eq!(
        book.attempt_purchase(
            &policy,
            &purchase,
            &witness,
            Route::Private,
            TradeOutcome::Succeeded
        ),
        Err(PurchaseError::AlreadyUsed)
    );
}

#[test]
fn a_permit_cannot_authorize_a_different_purchase_or_policy() {
    let (policy, purchase, witness) = public_purchase();
    let permit = check_eligibility(&policy, &purchase, &witness).unwrap();
    assert_eq!(check_public_binding(&policy, &purchase, &permit), Ok(()));
    for field in 0..8 {
        let mut changed_policy = policy.clone();
        let mut changed_purchase = purchase;
        match field {
            0 => changed_purchase.trading_account[0] ^= 1,
            1 => changed_purchase.recipient[0] ^= 1,
            2 => changed_purchase.collateral_in += 1,
            3 => changed_purchase.tokens_out += 1,
            4 => changed_policy.root[0] ^= 1,
            5 => changed_policy.collateral_limit += 1,
            6 => changed_policy.sale.namespace[0] ^= 1,
            7 => changed_policy.depth += 1,
            _ => unreachable!(),
        }
        assert!(check_public_binding(&changed_policy, &changed_purchase, &permit).is_err());
    }
}

#[test]
fn wrong_account_key_sale_root_or_membership_data_is_rejected() {
    for field in 0..8 {
        let (mut policy, purchase, mut witness) = public_purchase();
        match field {
            0 => witness.control = AccountControl::Public { key: [0; 32] },
            1 => {
                witness.control = AccountControl::Public {
                    key: digest("0000000000000000000000000000000000000000000000000000000000000004"),
                }
            }
            2 => witness.membership.blinder[0] ^= 1,
            3 => policy.root[0] ^= 1,
            4 => policy.sale.deployment[0] ^= 1,
            5 => policy.sale.factory[0] ^= 1,
            6 => policy.sale.namespace[0] ^= 1,
            7 => policy.sale.sale_account[0] ^= 1,
            _ => unreachable!(),
        }
        assert!(check_eligibility(&policy, &purchase, &witness).is_err());
    }
}

#[test]
fn private_accounts_with_one_wallet_key_have_separate_sale_allowances() {
    let (mut policy, purchase, mut witness) = public_purchase();
    let tree = MembershipTree::prepare(
        &policy.sale,
        TreeHash::Sha256,
        1,
        vec![
            ApprovedAccount {
                account: digest("a5342820e7ab710a41f19c489acf7ac00f2e32fd69a4595428bfb67740ff438e"),
                blinder: [5; 32],
            },
            ApprovedAccount {
                account: digest("cbc96df52836c30c37210056f541469c18f91a5f38f76379a5b6eaff137fbf48"),
                blinder: [5; 32],
            },
        ],
    )
    .unwrap();
    policy.root = tree.root();
    policy.depth = 1;
    let mut book = AllowanceBook::default();
    let mut permits = vec![];
    for identifier in 0..2 {
        witness.control = AccountControl::Private {
            nullifier_key: digest(
                "390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c",
            ),
            identifier,
        };
        witness.membership = tree.membership(identifier as u32).unwrap();
        permits.push(
            book.attempt_purchase(
                &policy,
                &purchase,
                &witness,
                Route::Private,
                TradeOutcome::Succeeded,
            )
            .unwrap(),
        );
    }
    assert_ne!(permits[0].nullifier, permits[1].nullifier);
}

// Circomlibjs 48b3ab3 reference vectors; see the research source record.
#[test]
fn poseidon_tree_and_membership_match_independent_circom_vectors() {
    let (mut policy, purchase, mut witness) = public_purchase();
    let tree = MembershipTree::prepare(
        &policy.sale,
        TreeHash::Poseidon,
        2,
        vec![
            ApprovedAccount {
                account: digest("ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8"),
                blinder: [5; 32],
            },
            ApprovedAccount {
                account: digest("a5342820e7ab710a41f19c489acf7ac00f2e32fd69a4595428bfb67740ff438e"),
                blinder: [5; 32],
            },
        ],
    )
    .unwrap();
    assert_eq!(
        tree.root(),
        digest("2c9a2523e437ba12ba9f1b9b08ddbbf7e2ae80ae656a45fa746a4164b5272cee")
    );
    policy.root = tree.root();
    policy.hash = TreeHash::Poseidon;
    policy.depth = 2;
    witness.membership = tree.membership(0).unwrap();
    assert!(check_eligibility(&policy, &purchase, &witness).is_ok());
    witness.control = AccountControl::Private {
        nullifier_key: digest("390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c"),
        identifier: 0,
    };
    witness.membership = tree.membership(1).unwrap();
    assert!(check_eligibility(&policy, &purchase, &witness).is_ok());
}

#[test]
fn poseidon_preserves_full_width_credential_bytes() {
    let (mut policy, purchase, mut witness) = public_purchase();
    policy.hash = TreeHash::Poseidon;
    policy.root = digest("17cb02affdc572edb2132fa0e5f06e476b0d43eb504de2a5f85f689a5473ec52");
    witness.membership.blinder = [255; 32];
    assert!(check_eligibility(&policy, &purchase, &witness).is_ok());
    witness.membership.blinder[31] = 254;
    assert!(check_eligibility(&policy, &purchase, &witness).is_err());
}

// LEZ a58fbce nullifier.rs supplies the private-account vector.
#[test]
fn approved_private_account_authorizes_the_bound_purchase() {
    let (mut policy, purchase, mut witness) = public_purchase();
    policy.root = digest("d29cf8dc06819098a9703ea750c10f30a9fa19b5f0acdae279b99dd394b419fb");
    witness.control = AccountControl::Private {
        nullifier_key: digest("390540739938b833cfee63a593d6d5971efb1ec48616e0d3ed7888e1bcdcf91c"),
        identifier: 0,
    };
    let expected = EligibilityPermit {
        sale: digest("deacaebd85d7044674c135bf0e9f43c63f1a22f165e9fd5c6fe715437e9dff5f"),
        root: policy.root,
        hash: TreeHash::Sha256,
        collateral_limit: 100,
        purchase: digest("5ae1a7eee33a3ea89d916b208267e5a0be430e21a54fca3fa357ec0ad2d3a934"),
        nullifier: digest("216a87db55c348245382affbee432e0f1a34fa44daa40bac67fa754ca82b76d9"),
    };
    assert_eq!(
        check_eligibility(&policy, &purchase, &witness),
        Ok(expected)
    );
}
