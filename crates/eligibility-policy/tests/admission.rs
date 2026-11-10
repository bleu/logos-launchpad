use eligibility_policy::{AllowlistPolicy, BuyPolicy, MembershipPath, VerifyError};

#[test]
fn allowlisted_trader_needs_a_membership_path() {
    let (policy, trader, _) = fixture();
    assert_eq!(policy.admit(&trader, None), Err(VerifyError::MissingPath));
}

#[test]
fn membership_can_be_reused_without_consuming_approval() {
    let (policy, trader, path) = fixture();
    for _ in 0..3 {
        assert_eq!(policy.admit(&trader, Some(&path)), Ok(()));
    }
}

#[test]
fn a_copied_path_does_not_admit_another_trader() {
    let (policy, _, path) = fixture();
    assert_eq!(
        policy.admit(&[9; 32], Some(&path)),
        Err(VerifyError::InvalidProof)
    );
}

#[test]
fn membership_is_bound_to_namespace_sale_policy_and_root() {
    let (BuyPolicy::Allowlist(policy), trader, path) = fixture() else {
        unreachable!()
    };
    for field in 0..3 {
        let mut changed = policy.clone();
        match field {
            0 => changed.namespace[0] ^= 1,
            1 => changed.sale_policy_id[0] ^= 1,
            _ => changed.root[0] ^= 1,
        }
        assert_eq!(
            BuyPolicy::Allowlist(changed).admit(&trader, Some(&path)),
            Err(VerifyError::InvalidProof)
        );
    }
}

#[test]
fn altered_membership_paths_are_rejected() {
    let (policy, trader, path) = fixture();
    for offset in [0, 1, 4, 35, 36, 68, 579] {
        let mut encoded = borsh::to_vec(&path).unwrap();
        encoded[offset] ^= 1;
        let changed = borsh::from_slice::<MembershipPath>(&encoded).unwrap();
        assert_eq!(
            policy.admit(&trader, Some(&changed)),
            Err(VerifyError::InvalidProof)
        );
    }
    let mut changed = path;
    changed.siblings.swap(0, 1);
    assert_eq!(
        policy.admit(&trader, Some(&changed)),
        Err(VerifyError::InvalidProof)
    );
}

#[test]
fn membership_rejects_unsupported_policy_versions() {
    let (BuyPolicy::Allowlist(mut policy), trader, path) = fixture() else {
        unreachable!()
    };
    for version in [0, 2, u8::MAX] {
        policy.version = version;
        assert_eq!(
            BuyPolicy::Allowlist(policy.clone()).admit(&trader, Some(&path)),
            Err(VerifyError::UnsupportedVersion)
        );
    }
}

#[test]
fn allowlist_binding_checks_the_creation_namespace_and_sale_policy() {
    let (policy, _, _) = fixture();
    assert_eq!(policy.validate_binding(&[2; 32], &[3; 32]), Ok(()));
    assert_eq!(
        policy.validate_binding(&[9; 32], &[3; 32]),
        Err(VerifyError::InvalidBinding)
    );
    assert_eq!(
        policy.validate_binding(&[2; 32], &[9; 32]),
        Err(VerifyError::InvalidBinding)
    );
}

#[test]
fn binding_rejects_unsupported_allowlist_versions() {
    let (BuyPolicy::Allowlist(mut policy), _, _) = fixture() else {
        unreachable!()
    };
    policy.version = 2;
    assert_eq!(
        BuyPolicy::Allowlist(policy).validate_binding(&[2; 32], &[3; 32]),
        Err(VerifyError::UnsupportedVersion)
    );
}

#[test]
fn open_policy_has_no_creation_binding_requirement() {
    assert_eq!(BuyPolicy::Open.validate_binding(&[9; 32], &[8; 32]), Ok(()));
}

#[test]
fn raw_verifier_accepts_the_independent_vector_without_an_enum_tag() {
    assert_eq!(
        eligibility_policy::verify_for(
            &vector_bytes("policyHex"),
            &vector_bytes("accountHex"),
            &vector_bytes("proofHex"),
        ),
        Ok(())
    );
}

#[test]
fn raw_verifier_rejects_noncanonical_lengths_before_decoding() {
    let records = [
        vector_bytes("policyHex"),
        vector_bytes("accountHex"),
        vector_bytes("proofHex"),
    ];
    for field in 0..3 {
        for replacement in [
            records[field][1..].to_vec(),
            [records[field].as_slice(), &[0]].concat(),
            vec![0; 10_000],
            Vec::new(),
        ] {
            let mut changed = records.clone();
            changed[field] = replacement;
            assert_eq!(
                eligibility_policy::verify_for(&changed[0], &changed[1], &changed[2]),
                Err(VerifyError::InvalidEncoding)
            );
        }
    }
}

#[test]
fn path_index_is_bounded_to_the_seventeen_level_tree() {
    let (policy, trader, mut path) = fixture();
    for index in [1 << 17, u32::MAX] {
        path.index = index;
        assert_eq!(
            policy.admit(&trader, Some(&path)),
            Err(VerifyError::InvalidEncoding)
        );
        assert_eq!(
            eligibility_policy::verify_for(
                &vector_bytes("policyHex"),
                &trader,
                &borsh::to_vec(&path).unwrap(),
            ),
            Err(VerifyError::InvalidEncoding)
        );
    }
    path.index = (1 << 17) - 1;
    assert_eq!(
        policy.admit(&trader, Some(&path)),
        Err(VerifyError::InvalidProof)
    );
}

#[test]
fn borsh_uses_fixed_records_and_explicit_policy_tags() {
    let (policy, _, path) = fixture();
    let BuyPolicy::Allowlist(allowlist) = &policy else {
        unreachable!()
    };
    let policy_bytes = vector_bytes("policyHex");
    let path_bytes = vector_bytes("proofHex");
    assert_eq!(policy_bytes.len(), 97);
    assert_eq!(path_bytes.len(), 580);
    assert_eq!(borsh::to_vec(allowlist).unwrap(), policy_bytes);
    assert_eq!(borsh::to_vec(&path).unwrap(), path_bytes);
    assert_eq!(borsh::to_vec(&BuyPolicy::Open).unwrap(), [0]);
    let tagged_policy = [vec![1], policy_bytes].concat();
    assert_eq!(borsh::to_vec(&policy).unwrap(), tagged_policy);
    assert_eq!(
        borsh::from_slice::<BuyPolicy>(&[0]).unwrap(),
        BuyPolicy::Open
    );
    assert_eq!(
        borsh::from_slice::<BuyPolicy>(&tagged_policy).unwrap(),
        policy
    );
}

#[test]
fn borsh_rejects_missing_unknown_truncated_and_trailing_policy_data() {
    let policy = [vec![1], vector_bytes("policyHex")].concat();
    for bytes in [
        Vec::new(),
        vec![2],
        vec![u8::MAX],
        vec![0, 0],
        policy[..policy.len() - 1].to_vec(),
        [policy.as_slice(), &[0]].concat(),
    ] {
        assert!(borsh::from_slice::<BuyPolicy>(&bytes).is_err());
    }
    for bytes in [vector_bytes("policyHex"), vector_bytes("proofHex")] {
        let truncated = &bytes[..bytes.len() - 1];
        let trailing = [bytes.as_slice(), &[0]].concat();
        if bytes.len() == 97 {
            assert!(borsh::from_slice::<AllowlistPolicy>(truncated).is_err());
            assert!(borsh::from_slice::<AllowlistPolicy>(&trailing).is_err());
        } else {
            assert!(borsh::from_slice::<MembershipPath>(truncated).is_err());
            assert!(borsh::from_slice::<MembershipPath>(&trailing).is_err());
        }
    }
}

fn vector_bytes(field: &str) -> Vec<u8> {
    let vector: serde_json::Value =
        serde_json::from_slice(include_bytes!("fixtures/merkle-vector.json")).unwrap();
    hex::decode(vector[field].as_str().unwrap()).unwrap()
}

fn fixture() -> (BuyPolicy, [u8; 32], MembershipPath) {
    let policy = borsh::from_slice::<AllowlistPolicy>(&vector_bytes("policyHex")).unwrap();
    let trader = vector_bytes("accountHex").try_into().unwrap();
    let path = borsh::from_slice(&vector_bytes("proofHex")).unwrap();
    (BuyPolicy::Allowlist(policy), trader, path)
}

#[test]
fn open_policy_admits_a_trader_without_a_path() {
    assert_eq!(BuyPolicy::Open.admit(&[9; 32], None), Ok(()));
}

#[test]
fn approved_trader_is_admitted_with_the_independent_sha256_vector() {
    let (policy, trader, path) = fixture();
    assert_eq!(policy.admit(&trader, Some(&path)), Ok(()));
}
