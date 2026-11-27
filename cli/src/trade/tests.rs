use super::{PriceQuote, QuoteMode};

#[test]
fn quote_json_values_stay_stable() {
    for (kind, expected) in [(QuoteMode::Buy, "buy"), (QuoteMode::Sell, "sell")] {
        let quote = PriceQuote {
            namespace: "namespace".to_owned(),
            protocol_fee_bps: 5,
            treasury: "treasury".to_owned(),
            kind,
            amount_in: 10,
            raw_amount_out: 9,
            amount_out: 8,
            protocol_fee: 1,
        };
        assert_eq!(serde_json::to_value(quote).unwrap()["kind"], expected);
    }
}
