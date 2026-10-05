use eligibility_core::{
    AccountControl, AllowanceBook, ApprovedAccount, EligibilityWitness, MembershipTree, Purchase,
    Route, SaleContext, SalePolicy, TradeOutcome, TreeHash,
};

#[test]
fn one_approved_account_has_a_separate_allowance_and_marker_for_each_sale() {
    let account = hex::decode("ec2cbcd4488075c4b30678c30351327fdf99f3fe703edc2999b68f8fd12bdbc8")
        .unwrap()
        .try_into()
        .unwrap();
    let mut key = [0; 32];
    key[31] = 3;
    let purchase = Purchase {
        trading_account: [6; 32],
        recipient: [7; 32],
        collateral_in: 10,
        tokens_out: 20,
    };
    let mut book = AllowanceBook::default();
    // Independently calculated with Python hashlib from the stated encodings.
    for (sale_account, marker) in [
        (
            [4; 32],
            "f8bcfd4fa8fefb511c2708862bffc01f0f2267af81c272ef630becd72770db13",
        ),
        (
            [8; 32],
            "6090bb142319f506a24f5a272c8578b0ed710d486292c37470763bd67eff93bc",
        ),
    ] {
        let sale = SaleContext {
            deployment: [1; 32],
            factory: [2; 32],
            namespace: [3; 32],
            sale_account,
        };
        let tree = MembershipTree::prepare(
            &sale,
            TreeHash::Sha256,
            0,
            vec![ApprovedAccount {
                account,
                blinder: [5; 32],
            }],
        )
        .unwrap();
        let policy = SalePolicy {
            sale,
            root: tree.root(),
            hash: TreeHash::Sha256,
            depth: 0,
            collateral_limit: 100,
        };
        let witness = EligibilityWitness {
            control: AccountControl::Public { key },
            membership: tree.membership(0).unwrap(),
        };
        let permit = book
            .attempt_purchase(
                &policy,
                &purchase,
                &witness,
                Route::Private,
                TradeOutcome::Succeeded,
            )
            .unwrap();
        assert_eq!(hex::encode(permit.nullifier), marker);
    }
}
