use std::{path::PathBuf, process::Command, time::Duration};
fn main() {
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let copy = args.first().is_some_and(|arg| arg == "--copy");
    if copy {
        args.remove(0);
    }
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
    }
    if args.len() != 1 {
        eprintln!("Usage: seele-fingerprint [--copy] -- FILE");
        std::process::exit(2);
    }
    let result = (|| {
        let cancel = seele_runtime::process::termination_signal()
            .map_err(|_| "Cannot establish cancellation.")?;
        let digest = seele_repo_tools::fingerprint::fingerprint(&PathBuf::from(&args[0]), &cancel)?;
        if copy {
            let mut command = if cfg!(target_os = "macos") {
                Command::new("pbcopy")
            } else {
                let mut c = Command::new("wl-copy");
                c.args(["--type", "text/plain;charset=utf-8"]);
                c
            };
            let reply = seele_runtime::process::discard_detaching(
                &mut command,
                digest.as_bytes(),
                seele_runtime::process::Limits {
                    timeout: Duration::from_secs(10),
                    output: 4096,
                },
                &cancel,
            )
            .map_err(|_| "Clipboard copy failed; file unchanged.")?;
            if !reply.success() {
                return Err("Clipboard copy failed; file unchanged.");
            }
        }
        println!("{digest}");
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
