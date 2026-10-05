fn main() {
    if let Err(error) =
        seele_config_tools::inspect::run(&std::env::args().skip(1).collect::<Vec<_>>())
    {
        eprintln!(
            "seele-inspect: {}",
            seele_config_tools::text::terminal(&error.to_string())
        );
        std::process::exit(2);
    }
}
