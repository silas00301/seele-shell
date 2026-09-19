fn main() {
    if let Err(error) = seele_config_tools::themes::main() {
        eprintln!("Seele Themes: {error}");
        std::process::exit(1);
    }
}
