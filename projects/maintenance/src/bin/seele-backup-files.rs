use std::io::Write;
fn main() {
    let result = (|| -> std::io::Result<()> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let cancel = seele_runtime::process::termination_signal()?;
        let backend = seele_maintenance::restic_files::Backend::from_env()?;
        let bytes = match args.as_slice() {
            [mode, path] if mode == "versions" => {
                serde_json::to_vec(&backend.versions(std::path::Path::new(path), &cancel)?)?
            }
            [mode, snapshot, path] if mode == "dump" => {
                backend.dump(snapshot, std::path::Path::new(path), &cancel)?
            }
            _ => return Err(std::io::ErrorKind::InvalidInput.into()),
        };
        std::io::stdout().lock().write_all(&bytes)
    })();
    if result.is_err() {
        eprintln!("Backup version unavailable; no originals were changed.");
        std::process::exit(1)
    }
}
