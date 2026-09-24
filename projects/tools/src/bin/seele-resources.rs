//! Open-panel-only, read-only resource sampler; no command lines or persistence.
#[path = "../resources/mod.rs"]
mod resources;
fn main() {
    if resources::run().is_err() {
        std::process::exit(1);
    }
}
