#[path = "setup/allowlist.rs"]
mod allowlist;
#[path = "setup/buys.rs"]
mod buys;
#[path = "setup/controls.rs"]
mod controls;
#[path = "setup/creation.rs"]
mod creation;
#[path = "setup/events.rs"]
mod events;
#[path = "setup/fixture.rs"]
mod fixture;
#[path = "setup/sells.rs"]
mod sells;
#[path = "setup/settlement.rs"]
mod settlement;

use self::fixture::{exact_in, id};
use lbp_core::Message;

#[test]
fn actor_message_tags_keep_setup_and_read_state_order() {
    for (message, tag) in [
        (Message::ActivateFixedPrice, 1),
        (Message::ReadState, 2),
        (exact_in(5), 3),
        (
            Message::BuyExactOutput {
                membership: None,
                amount_out: 3,
                max_amount_in: 5,
                now: 100,
            },
            4,
        ),
        (Message::CollectFees, 5),
        (
            Message::Execute {
                pool: id(50),
                instruction: Box::new(exact_in(5)),
            },
            6,
        ),
    ] {
        assert_eq!(borsh::to_vec(&message).unwrap()[0], tag);
    }
}
