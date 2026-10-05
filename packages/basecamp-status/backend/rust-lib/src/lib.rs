pub trait LaunchpadStatusCoreModule: Send + 'static {
    fn get_status(&mut self) -> String;

    fn on_context_ready(&mut self, _ctx: &RustModuleContext) {}
}

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/generated/provider_gen.rs"
));

#[derive(Default)]
struct Status;

impl LaunchpadStatusCoreModule for Status {
    fn get_status(&mut self) -> String {
        launchpad_status::get_status()
    }
}

#[no_mangle]
pub extern "Rust" fn logos_module_install() {
    install::<Status>();
}
