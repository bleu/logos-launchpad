fn main() {
    println!("cargo:rerun-if-env-changed=RISC0_SKIP_BUILD");
    println!("cargo:rerun-if-env-changed=RISC0_BUILD_LOCKED");
    risc0_build::embed_methods();
}
