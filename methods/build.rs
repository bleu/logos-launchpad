#![cfg_attr(not(test), warn(unused_crate_dependencies))]

fn main() {
    println!("cargo:rerun-if-env-changed=CURVE_EMERGENCY_AUTHORITY");
    println!("cargo:rerun-if-env-changed=LAUNCHPAD_MOCK_CLOCK");
    risc0_build::embed_methods();
}
