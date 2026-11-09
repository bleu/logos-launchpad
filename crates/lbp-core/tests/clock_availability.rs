#[test]
fn clock_provider_requires_explicit_local_build() {
    assert_eq!(
        lbp_core::pricing_clock::require_available().is_ok(),
        cfg!(local_mock_clock)
    );
}
