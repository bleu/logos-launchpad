pub trait LaunchpadCoreModule: Send + 'static {
    fn get_status(&mut self) -> String;

    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/generated/provider_gen.rs"
));

#[derive(Default)]
struct Core;

impl LaunchpadCoreModule for Core {
    fn get_status(&mut self) -> String {
        launchpad_core_logic::get_status()
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<Core>();
}
