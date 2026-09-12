fn main() {
    if std::env::args_os().len() != 1 {
        eprintln!("Usage: reboot-windows-service");
        std::process::exit(2);
    }
    let result: seele_runtime::Result = (|| {
        let cancel = seele_runtime::process::termination_signal()?;
        seele_desktop_tools::windows::service(&cancel)
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
