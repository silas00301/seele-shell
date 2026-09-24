//! Quick Look's resident worker.
//!
//! It links only the shared runtime and Poppler's command-line tools, never
//! the desktop tools library, so classifying a highlighted file carries no
//! D-Bus, audio or recognition code.
#[path = "../quicklook/mod.rs"]
mod quicklook;

fn main() {
    if let Err(error) = quicklook::run() {
        eprintln!("seele-quicklook: {error}");
        std::process::exit(1);
    }
}
