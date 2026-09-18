fn main() {
    if let Err(error) =
        seele_desktop_tools::firmware::run(&std::env::args().skip(1).collect::<Vec<_>>())
    {
        eprintln!("seele-firmware-check: {error}");
        std::process::exit(1);
    }
}
