#[path = "weighted_trading/allowlist.rs"]
mod allowlist;
#[path = "weighted_trading/buys.rs"]
mod buys;
#[path = "weighted_trading/checkpoints.rs"]
mod checkpoints;
#[path = "weighted_trading/clock.rs"]
mod clock;
#[path = "weighted_trading/controls.rs"]
mod controls;
#[path = "weighted_trading/events.rs"]
mod events;
#[path = "weighted_trading/fixture.rs"]
mod fixture;
#[path = "weighted_trading/sells.rs"]
mod sells;
#[path = "weighted_trading/settlement.rs"]
mod settlement;

use self::fixture::{id, weighted_inputs};
use lbp_core::compute_pool_pda;
use lee_core::account::AccountId;
use lee_core::program::PdaSeed;

#[test]
fn weighted_message_tags_follow_fixed_price_variants() {
    // Fixed-price tags 0-7 must be unchanged; weighted variants come after.
    use lbp_core::Message;
    let factory = AccountId::for_public_pda(&id(7), &PdaSeed::new([3; 32]));
    let pool = compute_pool_pda(id(40), id(8), id(41), id(42), factory);
    for (msg, expected_tag) in [
        (
            Message::PrepareWeighted {
                inputs: weighted_inputs(),
            },
            8u8,
        ),
        (Message::ActivateWeighted, 9),
    ] {
        assert_eq!(
            borsh::to_vec(&msg).unwrap()[0],
            expected_tag,
            "wrong tag for {msg:?}"
        );
        let _ = pool;
    }
}
