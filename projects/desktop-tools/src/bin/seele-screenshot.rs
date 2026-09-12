#[cfg(target_os = "linux")]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() > 1
        || args
            .first()
            .is_some_and(|mode| !matches!(mode.as_str(), "capture" | "annotate" | "upload"))
    {
        eprintln!("Usage: seele-screenshot [capture|annotate|upload]");
        std::process::exit(2);
    }
    let result = seele_runtime::process::termination_signal().and_then(|cancel| {
        seele_desktop_tools::screenshot::run(
            args.first().map_or("capture", String::as_str),
            &cancel,
        )
    });
    if result.is_err() {
        eprintln!("Screenshot operation failed; completed captures remain local.");
        std::process::exit(1);
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Screenshots require the Linux Wayland desktop");
    std::process::exit(2);
}
