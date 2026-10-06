#[cfg(target_os = "linux")]
fn main() {
    if std::env::args_os().len() != 1 {
        eprintln!("Usage: seele-record");
        std::process::exit(2)
    }
    let result = seele_runtime::process::termination_signal()
        .and_then(|cancel| seele_desktop_tools::recording::run(&cancel));
    if result.is_err() {
        eprintln!("Recording operation failed; saved originals remain local.");
        std::process::exit(1)
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Recording requires Linux Wayland");
    std::process::exit(2)
}
