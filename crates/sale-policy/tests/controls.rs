use sale_policy::{PauseError, PausePolicy, TradeControl};

#[test]
fn disabled_controls_never_stop_trades_or_change_policy() {
    let mut control = TradeControl::new(PausePolicy::Disabled);
    for paused in [false, true, false] {
        assert_eq!(control.set_paused(paused), Err(PauseError::Disabled));
        assert_eq!(control.policy(), PausePolicy::Disabled);
        assert!(!control.is_paused());
    }
}

#[test]
fn repeated_controls_report_only_real_changes_and_keep_policy() {
    let mut control = TradeControl::new(PausePolicy::AuthorityControlled);
    for (paused, changed) in [
        (false, false),
        (true, true),
        (true, false),
        (false, true),
        (false, false),
    ] {
        assert_eq!(control.set_paused(paused), Ok(changed));
        assert_eq!(control.is_paused(), paused);
        assert_eq!(control.policy(), PausePolicy::AuthorityControlled);
    }
}
