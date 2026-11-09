#[test]
fn normal_build_requests_the_canonical_clock_at_full_precision() {
    assert_eq!(lbp_core::pricing_clock::actor(), clock_core::clock_actor());
    assert_eq!(
        lbp_core::pricing_clock::request(),
        clock_core::Message::ReadTimestamp { granularity: 1 }
    );
}
