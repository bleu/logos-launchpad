use launchpad_core_logic::get_status;

#[test]
fn backend_reports_ok() {
    assert_eq!(get_status(), "Backend OK");
}
