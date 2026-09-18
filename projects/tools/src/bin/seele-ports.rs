//! The port inspector's resident, unprivileged worker.
//!
//! It links only `/proc` reading and the shared runtime, never the desktop
//! tools library, so discovery carries no D-Bus, image or recognition code.
#[path = "../ports/model.rs"]
mod model;
#[path = "../ports/procfs.rs"]
mod procfs;
#[path = "../ports/worker.rs"]
mod worker;

fn main() {
    worker::run();
}
