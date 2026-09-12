fn main() {
    if std::env::args_os().len() != 1 {
        eprintln!("Usage: set-brave-qt-theme");
        std::process::exit(2);
    }
    #[cfg(target_os = "linux")]
    if let Some(root) = seele_desktop_tools::brave::configured_root() {
        if seele_desktop_tools::brave::update(&root).is_err() {
            eprintln!("Brave theme preferences were left unchanged.");
        }
    }
}
