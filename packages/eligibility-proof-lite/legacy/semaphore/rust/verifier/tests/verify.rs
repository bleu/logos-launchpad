use ark_bn254::{Fq, Fq2, G2Affine};
use ark_ec::AffineRepr;
use eligibility_proof_verifier::{verify, verify_for, VerifyError};

const FIXTURE: &[u8] = include_bytes!("../../../fixtures/proof.json");
const EXPECTED: [&str; 4] = [
    "7173586477429286536749989773509006115088897804557537214051884260895089866472",
    "3722214579166817022322923951306289336744565431242118448493274318889200899032",
    "337128325429352729837209583172397910712856832050213866488156768494212314437",
    "294047424595278025196507086131157496510339541055861277577031885441431189190",
];

#[test]
fn expected_public_inputs_bind_each_signal() {
    assert_eq!(verify_for(FIXTURE, &EXPECTED), Ok(()));
    for index in 0..4 {
        let mut expected = EXPECTED;
        expected[index] = "1";
        assert!(
            verify_for(FIXTURE, &expected).is_err(),
            "unbound signal {index}"
        );
    }
}

#[test]
fn official_semaphore_proof_passes() {
    assert_eq!(verify(FIXTURE), Ok(()));
}

#[test]
fn changed_public_inputs_fail_the_pairing_check() {
    for index in 0..4 {
        let mut request: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        request["publicSignals"][index] = "1".into();
        assert_eq!(
            verify(&serde_json::to_vec(&request).unwrap()),
            Err(VerifyError::InvalidProof)
        );
    }
}

#[test]
fn proof_coordinates_require_canonical_base_field_decimals() {
    for path in [
        "/proof/pi_a/0",
        "/proof/pi_a/1",
        "/proof/pi_b/0/0",
        "/proof/pi_b/0/1",
        "/proof/pi_b/1/0",
        "/proof/pi_b/1/1",
        "/proof/pi_c/0",
        "/proof/pi_c/1",
    ] {
        for value in [
            "01",
            "-1",
            "+1",
            " 1",
            "0x01",
            "",
            "21888242871839275222246405745257275088696311157297823662689037894645226208583",
        ] {
            let mut request: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
            *request.pointer_mut(path).unwrap() = value.into();
            assert_eq!(
                verify(&serde_json::to_vec(&request).unwrap()),
                Err(VerifyError::InvalidEncoding),
                "accepted {path}: {value}"
            );
        }
    }
}

#[test]
fn public_input_counts_and_types_are_exact() {
    for signals in [
        serde_json::json!(["1", "2", "3"]),
        serde_json::json!(["1", "2", "3", "4", "5"]),
        serde_json::json!([1, "2", "3", "4"]),
    ] {
        let mut request: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
        request["publicSignals"] = signals;
        assert_eq!(
            verify(&serde_json::to_vec(&request).unwrap()),
            Err(VerifyError::MalformedJson)
        );
    }
}

#[test]
fn snarkjs_unit_projective_tails_pass_but_other_tails_fail() {
    let mut request: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    request["proof"]["pi_a"]
        .as_array_mut()
        .unwrap()
        .push("1".into());
    request["proof"]["pi_b"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!(["1", "0"]));
    request["proof"]["pi_c"]
        .as_array_mut()
        .unwrap()
        .push("1".into());
    assert_eq!(verify(&serde_json::to_vec(&request).unwrap()), Ok(()));
    for path in ["/proof/pi_a/2", "/proof/pi_b/2/0", "/proof/pi_c/2"] {
        let mut changed = request.clone();
        *changed.pointer_mut(path).unwrap() = "0".into();
        assert_eq!(
            verify(&serde_json::to_vec(&changed).unwrap()),
            Err(VerifyError::InvalidEncoding)
        );
    }
}

#[test]
fn proof_coordinate_counts_are_exact() {
    for name in ["pi_a", "pi_b", "pi_c"] {
        for count in [0, 1, 4] {
            let mut request: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
            let coordinate = if name == "pi_b" {
                serde_json::json!(["1", "0"])
            } else {
                serde_json::json!("1")
            };
            request["proof"][name] = serde_json::Value::Array(vec![coordinate; count]);
            assert_eq!(
                verify(&serde_json::to_vec(&request).unwrap()),
                Err(VerifyError::InvalidEncoding)
            );
        }
    }
}

#[test]
fn on_curve_g2_points_outside_the_subgroup_are_rejected() {
    let point = (0..100)
        .find_map(|x| {
            G2Affine::get_point_from_x_unchecked(Fq2::new(Fq::from(x), Fq::from(1)), false)
                .filter(|point| !point.is_in_correct_subgroup_assuming_on_curve())
        })
        .unwrap();
    let mut request: serde_json::Value = serde_json::from_slice(FIXTURE).unwrap();
    request["proof"]["pi_b"] = serde_json::json!([
        [point.x.c0.to_string(), point.x.c1.to_string()],
        [point.y.c0.to_string(), point.y.c1.to_string()]
    ]);
    assert_eq!(
        verify(&serde_json::to_vec(&request).unwrap()),
        Err(VerifyError::InvalidPoint)
    );
}

#[test]
fn replay_passes_without_external_nullifier_state() {
    assert_eq!(verify_for(FIXTURE, &EXPECTED), Ok(()));
    assert_eq!(verify_for(FIXTURE, &EXPECTED), Ok(()));
}

#[test]
fn malformed_requests_return_errors() {
    for input in [b"{".as_slice(), b"null", b"{}", b"[]", b"\xff"] {
        assert_eq!(verify(input), Err(VerifyError::MalformedJson));
    }
}

#[test]
fn public_inputs_require_canonical_scalar_decimals() {
    let mut request = serde_json::json!({
        "proof": {
            "pi_a": ["0", "0", "1"],
            "pi_b": [["0", "0"], ["0", "0"], ["1", "0"]],
            "pi_c": ["0", "0", "1"],
            "protocol": "groth16", "curve": "bn128"
        },
        "publicSignals": ["0", "0", "0", "0"]
    });
    for value in [
        "01",
        "+1",
        "-1",
        " 1",
        "0x01",
        "",
        "21888242871839275222246405745257275088548364400416034343698204186575808495617",
    ] {
        request["publicSignals"][0] = value.into();
        assert_eq!(
            verify(&serde_json::to_vec(&request).unwrap()),
            Err(VerifyError::InvalidEncoding),
            "accepted {value}"
        );
    }
}

#[test]
fn off_curve_proof_points_are_rejected() {
    let request = serde_json::json!({
        "proof": {
            "pi_a": ["0", "0", "1"],
            "pi_b": [["0", "0"], ["0", "0"], ["1", "0"]],
            "pi_c": ["0", "0", "1"],
            "protocol": "groth16", "curve": "bn128"
        },
        "publicSignals": ["0", "0", "0", "0"]
    });
    assert!(verify(&serde_json::to_vec(&request).unwrap()).is_err());
}

#[test]
fn curve_points_alone_are_not_a_valid_proof() {
    let b = G2Affine::generator();
    let request = serde_json::json!({
        "proof": {
            "pi_a": ["1", "2", "1"],
            "pi_b": [[b.x.c0.to_string(), b.x.c1.to_string()], [b.y.c0.to_string(), b.y.c1.to_string()], ["1", "0"]],
            "pi_c": ["1", "2", "1"],
            "protocol": "groth16", "curve": "bn128"
        },
        "publicSignals": ["0", "0", "0", "0"]
    });
    assert!(verify(&serde_json::to_vec(&request).unwrap()).is_err());
}
