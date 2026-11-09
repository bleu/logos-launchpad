fn main() {
    println!("cargo:rerun-if-env-changed=LAUNCHPAD_MOCK_CLOCK");
    println!("cargo:rustc-check-cfg=cfg(local_mock_clock)");
    if std::env::var("LAUNCHPAD_MOCK_CLOCK").as_deref() == Ok("1") {
        println!("cargo:rustc-cfg=local_mock_clock");
    }
}
