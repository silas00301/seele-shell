//! Existing EFI boot selection with fixed paths and bounded subprocesses.
use seele_runtime::{process, Result};
use std::os::unix::fs::MetadataExt;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::AtomicUsize,
    time::Duration,
};
fn trusted(name: &str) -> Result<PathBuf> {
    let value = std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or("Missing packaged executable")?;
    let path = value.canonicalize()?;
    let metadata = path.metadata()?;
    if !metadata.is_file() || metadata.mode() & 0o111 == 0 {
        return Err("Invalid packaged executable".into());
    }
    for ancestor in path.ancestors() {
        let meta = ancestor.metadata()?;
        if meta.uid() != 0 || meta.mode() & 0o022 != 0 {
            return Err("Unsafe packaged executable".into());
        }
    }
    Ok(path)
}
fn command(path: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(path);
    command
        .args(args)
        .env_clear()
        .env("LC_ALL", "C")
        .env("PATH", "");
    command
}
fn run(path: &Path, args: &[&str], cancel: &AtomicUsize) -> Result<Vec<u8>> {
    let output = process::capture(
        &mut command(path, args),
        &[],
        process::Limits {
            timeout: Duration::from_secs(5),
            output: 256 * 1024,
        },
        cancel,
    )?;
    if !output.status.success() {
        return Err("Boot service command failed".into());
    }
    Ok(output.stdout)
}
fn selection(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes)?;
    let mut windows = None;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("Boot") else {
            continue;
        };
        if rest.len() < 5 {
            continue;
        }
        let Some(number) = rest.get(..4) else {
            continue;
        };
        if !number.bytes().all(|c| c.is_ascii_hexdigit()) {
            continue;
        }
        let rest = &rest[4..];
        let rest = rest.strip_prefix('*').unwrap_or(rest);
        if !rest.starts_with(|c: char| c.is_ascii_whitespace()) {
            continue;
        }
        let Some(tail) = rest.trim_start().strip_prefix("Windows Boot Manager") else {
            continue;
        };
        if tail.is_empty() || tail.starts_with(|c: char| c.is_ascii_whitespace()) {
            windows.get_or_insert_with(|| number.to_owned());
        }
    }
    windows.ok_or_else(|| "Windows Boot Manager EFI entry not found".into())
}
fn reboot(efi: &Path, systemctl: &Path, cancel: &AtomicUsize) -> Result<()> {
    let target = selection(&run(efi, &[], cancel)?)?;
    run(efi, &["--bootnext", &target], cancel)?;
    run(systemctl, &["--no-block", "reboot"], cancel)?;
    Ok(())
}
pub fn service(cancel: &AtomicUsize) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err("Windows boot selection requires its system service".into());
    }
    reboot(
        &trusted("SEELE_EFIBOOTMGR")?,
        &trusted("SEELE_SYSTEMCTL")?,
        cancel,
    )
}
pub fn request(cancel: &AtomicUsize) -> Result<()> {
    run(
        &trusted("SEELE_SYSTEMCTL")?,
        &["--no-block", "start", "reboot-windows.service"],
        cancel,
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt};
    #[test]
    fn parses_first_exact_windows_label_and_rejects_malformed_entries() {
        assert_eq!(selection(b"BootNext: 0001\nBoot00aF* Windows Boot Manager\tHD(data)\nBoot0002 Windows Boot Manager\n").unwrap(),"00aF");
        for text in [
            "Boot000g Windows Boot Manager",
            "Boot0001 Windows Boot ManagerEvil",
            "Boot0001Windows Boot Manager",
            "BootAAA🦀 Windows Boot Manager",
            "Boot0001\u{a0}Windows Boot Manager",
        ] {
            assert!(selection(text.as_bytes()).is_err());
        }
        assert!(selection(&[255]).is_err());
    }
    fn quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
    fn script(path: &Path, body: &str) {
        let shell = std::env::var_os("PATH")
            .and_then(|paths| {
                std::env::split_paths(&paths)
                    .map(|p| p.join("sh"))
                    .find(|p| p.is_file())
            })
            .unwrap();
        fs::write(path, format!("#!{}\n{body}\n", shell.display())).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[test]
    fn synthetic_commands_preserve_order_and_fail_before_later_actions() {
        let temp = tempfile::tempdir().unwrap();
        let efi = temp.path().join("efi");
        let system = temp.path().join("systemctl");
        let log = temp.path().join("calls");
        let log = quote(log.to_str().unwrap());
        script(&efi,&format!("printf 'efi %s\\n' \"$*\" >>{log}\nif [ \"$#\" -eq 0 ]; then printf 'Boot0001* Windows Boot Manager\\n'; fi"));
        script(&system, &format!("printf 'systemctl %s\\n' \"$*\" >>{log}"));
        reboot(&efi, &system, &AtomicUsize::new(0)).unwrap();
        let logfile = temp.path().join("calls");
        assert_eq!(
            fs::read_to_string(&logfile).unwrap(),
            "efi \nefi --bootnext 0001\nsystemctl --no-block reboot\n"
        );
        fs::write(&logfile, "").unwrap();
        script(&efi,&format!("printf 'efi %s\\n' \"$*\" >>{log}\nif [ \"$#\" -eq 0 ]; then printf 'Boot0001* Windows Boot Manager\\n'; else exit 7; fi"));
        assert!(reboot(&efi, &system, &AtomicUsize::new(0)).is_err());
        assert_eq!(
            fs::read_to_string(&logfile).unwrap(),
            "efi \nefi --bootnext 0001\n"
        );
        fs::write(&logfile, "").unwrap();
        assert!(reboot(&efi, &system, &AtomicUsize::new(15)).is_err());
        assert_eq!(fs::read_to_string(&logfile).unwrap(), "");
        script(&efi, "printf 'Boot0001* Linux\\n'");
        assert!(reboot(&efi, &system, &AtomicUsize::new(0)).is_err());
        assert_eq!(fs::read_to_string(&logfile).unwrap(), "");
    }
}
