//! The port inspector's privileged helper.
//!
//! It is executed through `run0` with a typed target and nothing else. It
//! links only `/proc` reading and the shared runtime, so the program that runs
//! as root stays as small as the decision it has to re-check.
#[path = "../ports/privileged.rs"]
mod privileged;
#[path = "../ports/procfs.rs"]
mod procfs;

fn main() -> std::process::ExitCode {
    privileged::main()
}
