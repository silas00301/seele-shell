fn run() -> seele_config_tools::Result {
    let mut args = std::env::args().skip(1);
    let manifest = args.next().ok_or("missing catalog manifest")?;
    seele_config_tools::catalog::run(manifest.as_ref(), &args.collect::<Vec<_>>())
}
fn main() {
    if let Err(error) = run() {
        eprintln!(
            "seele-portable-apps: {}",
            seele_config_tools::text::terminal(&error.to_string())
        );
        std::process::exit(2);
    }
}
