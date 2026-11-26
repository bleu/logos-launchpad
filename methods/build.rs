#![cfg_attr(not(test), warn(unused_crate_dependencies))]

fn main() {
    println!("cargo:rerun-if-env-changed=CURVE_EMERGENCY_AUTHORITY");
    if cfg!(feature = "standalone-test") {
        println!("cargo:rerun-if-env-changed=LAUNCHPAD_AUTHORITY_PROGRAM_ACCOUNT");
        let mut guest = risc0_build::GuestOptions::default();
        guest.features.push("standalone-test".into());
        risc0_build::embed_methods_with_options([("launchpad_programs", guest)].into());
    } else {
        risc0_build::embed_methods();
    }
}
