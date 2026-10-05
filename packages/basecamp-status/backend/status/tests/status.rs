use launchpad_status::get_status;

#[test]
fn backend_reports_ok() {
    assert_eq!(get_status(), "Backend OK");
}
