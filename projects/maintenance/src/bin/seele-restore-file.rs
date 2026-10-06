use seele_maintenance::restic_files::{current, FILE_LIMIT};
use seele_runtime::process::{capture, Limits};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;
fn dialog(args: &[String], input: &[u8], cancel: &AtomicUsize) -> io::Result<Option<String>> {
    let out = capture(
        Command::new("zenity").args(args),
        input,
        Limits {
            timeout: Duration::from_secs(900),
            output: 8192,
        },
        cancel,
    )?;
    if !out.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8(out.stdout)
            .map_err(|_| io::ErrorKind::InvalidData)?
            .trim_end_matches('\n')
            .into(),
    ))
}
fn helper(args: &[&str], cancel: &AtomicUsize) -> io::Result<Vec<u8>> {
    let program = PathBuf::from(
        std::env::var_os("SEELE_BACKUP_FILES_HELPER").ok_or(io::ErrorKind::NotFound)?,
    );
    let canonical = program.canonicalize()?;
    let metadata = fs::metadata(&canonical)?;
    if !canonical.starts_with("/nix/store")
        || canonical.file_name() != Some(std::ffi::OsStr::new("seele-backup-files"))
        || !metadata.is_file()
        || metadata.uid() != 0
        || metadata.mode() & 0o022 != 0
        || metadata.mode() & 0o111 == 0
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    helper_command("/run/current-system/sw/bin/run0", &canonical, args, cancel)
}
fn helper_command(
    run0: &str,
    program: &Path,
    args: &[&str],
    cancel: &AtomicUsize,
) -> io::Result<Vec<u8>> {
    let out = capture(
        Command::new(run0)
            .args(["--pipe", "--property=RuntimeMaxSec=180", "--"])
            .arg(program)
            .args(args),
        b"",
        Limits {
            timeout: Duration::from_secs(185),
            output: FILE_LIMIT + 65536,
        },
        cancel,
    )?;
    if !out.status.success() {
        return Err(io::Error::other("backup version unavailable"));
    }
    Ok(out.stdout)
}
fn view(text: &str, title: &str, cancel: &AtomicUsize) -> io::Result<()> {
    dialog(
        &[
            "--text-info".into(),
            format!("--title={title}"),
            "--width=760".into(),
            "--height=560".into(),
        ],
        text.as_bytes(),
        cancel,
    )?;
    Ok(())
}
fn safe_text(text: &str) -> String {
    text.chars().filter(|c|matches!(c,'\n'|'\t') || (!c.is_control() && !matches!(*c,'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}'))).collect()
}
fn compare(path: &Path, backup: &[u8], stage: &Path, cancel: &AtomicUsize) -> io::Result<()> {
    let live=match current(path){Ok(bytes)=>bytes,Err(_)=>return view("The current file is unavailable or exceeds the 16 MiB comparison bound. The backup remains available for preview or a restored copy.","Compare versions",cancel)};
    let header = format!(
        "Current: {} bytes, SHA-256 {:x}\nBackup: {} bytes, SHA-256 {:x}\n\n",
        live.len(),
        Sha256::digest(&live),
        backup.len(),
        Sha256::digest(backup)
    );
    if live == backup {
        return view(
            &(header + "The bytes are identical."),
            "Compare versions",
            cancel,
        );
    }
    if live.len() <= 1024 * 1024
        && backup.len() <= 1024 * 1024
        && !live.contains(&0)
        && !backup.contains(&0)
        && std::str::from_utf8(&live).is_ok()
        && std::str::from_utf8(backup).is_ok()
    {
        let mut current =
            tempfile::NamedTempFile::new_in(stage.parent().ok_or(io::ErrorKind::InvalidInput)?)?;
        current
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
        current.write_all(&live)?;
        let out = capture(
            Command::new("diff")
                .args(["-u", "--label", "Backup", "--label", "Current", "--"])
                .arg(stage)
                .arg(current.path()),
            b"",
            Limits {
                timeout: Duration::from_secs(5),
                output: 256 * 1024,
            },
            cancel,
        )?;
        if matches!(out.status.code(), Some(0 | 1)) {
            return view(
                &(header + &safe_text(&String::from_utf8_lossy(&out.stdout))),
                "Compare versions",
                cancel,
            );
        }
    }
    view(&(header+"These bytes differ. Text diffs are available for UTF-8 files up to 1 MiB; larger or binary files show hashes and sizes."),"Compare versions",cancel)
}
fn run(
    args: &[String],
    helper: fn(&[&str], &AtomicUsize) -> io::Result<Vec<u8>>,
) -> io::Result<()> {
    let cancel = seele_runtime::process::termination_signal()?;
    if args.len() > 1 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let path = if let Some(path) = args.first() {
        PathBuf::from(path)
    } else {
        let Some(path)=dialog(&["--entry".into(),"--title=Restore a backup version".into(),"--text=Enter the original absolute file path in your home. Deleted files can be looked up too. Originals will not be overwritten.".into()],b"",&cancel)? else{return Ok(())};
        PathBuf::from(path)
    };
    let bytes = helper(
        &[
            "versions",
            path.to_str().ok_or(io::ErrorKind::InvalidInput)?,
        ],
        &cancel,
    )?;
    let rows: Vec<Value> = serde_json::from_slice(&bytes)?;
    if rows.is_empty() {
        return view("No backed-up regular-file versions at most 16 MiB were found for this exact path on the configured host.","Backup versions",&cancel);
    }
    if rows.len() > 256 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut choices = vec![
        "--list".into(),
        "--title=Choose a backup version".into(),
        "--column=Choice".into(),
        "--column=Modified".into(),
        "--column=Bytes".into(),
        "--column=Snapshot".into(),
        "--print-column=1".into(),
    ];
    for (index, row) in rows.iter().enumerate() {
        let id = row["snapshot"]
            .as_str()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or(io::ErrorKind::InvalidData)?;
        choices.extend([
            index.to_string(),
            safe_text(row["modified"].as_str().unwrap_or("Unknown")),
            row["size"]
                .as_u64()
                .ok_or(io::ErrorKind::InvalidData)?
                .to_string(),
            id[..12].to_owned(),
        ]);
    }
    let runtime =
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or(io::ErrorKind::NotFound)?);
    let meta = fs::symlink_metadata(&runtime)?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let temp = tempfile::Builder::new()
        .prefix("seele-restore-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(runtime)?;
    loop {
        let Some(selected) = dialog(&choices, b"", &cancel)? else {
            return Ok(());
        };
        let row = rows
            .get(
                selected
                    .parse::<usize>()
                    .map_err(|_| io::ErrorKind::InvalidData)?,
            )
            .ok_or(io::ErrorKind::InvalidData)?;
        let snapshot = row["snapshot"].as_str().ok_or(io::ErrorKind::InvalidData)?;
        let bytes = helper(
            &[
                "dump",
                snapshot,
                path.to_str().ok_or(io::ErrorKind::InvalidInput)?,
            ],
            &cancel,
        )?;
        if bytes.len() > FILE_LIMIT
            || bytes.len() as u64 != row["size"].as_u64().ok_or(io::ErrorKind::InvalidData)?
        {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let name = path.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        let stage = temp.path().join(name);
        fs::write(&stage, &bytes)?;
        fs::set_permissions(&stage, fs::Permissions::from_mode(0o600))?;
        loop {
            let Some(action) = dialog(
                &[
                    "--list".into(),
                    "--title=Backup version".into(),
                    "--column=Action".into(),
                    "Preview".into(),
                    "Compare with current".into(),
                    "Restore a copy".into(),
                    "Choose another version".into(),
                ],
                b"",
                &cancel,
            )?
            else {
                return Ok(());
            };
            match action.as_str() {
                "Preview" => {
                    let shellctl =
                        std::env::var_os("SEELE_SHELLCTL").ok_or(io::ErrorKind::NotFound)?;
                    let result = capture(
                        Command::new(shellctl).arg("quicklook").arg(&stage),
                        b"",
                        Limits {
                            timeout: Duration::from_secs(5),
                            output: 4096,
                        },
                        &cancel,
                    )?;
                    if !result.status.success() {
                        return Err(io::Error::other("preview unavailable"));
                    }
                    // Retain the private file while the user is looking. The
                    // existing Quick Look layer owns its keyboard and dismissal.
                    dialog(
                        &[
                            "--info".into(),
                            "--title=Backup preview".into(),
                            "--text=Close Quick Look, then press OK to return to backup versions."
                                .into(),
                        ],
                        b"",
                        &cancel,
                    )?;
                }
                "Compare with current" => compare(&path, &bytes, &stage, &cancel)?,
                "Restore a copy" => {
                    let suggested = path.with_file_name(format!(
                        "{}-restored-{}",
                        name.to_string_lossy(),
                        &snapshot[..12]
                    ));
                    let Some(destination) = dialog(
                        &[
                            "--file-selection".into(),
                            "--save".into(),
                            "--title=Save a restored copy".into(),
                            format!("--filename={}", suggested.display()),
                        ],
                        b"",
                        &cancel,
                    )?
                    else {
                        continue;
                    };
                    let destination = PathBuf::from(destination);
                    if !destination.is_absolute() || destination == path {
                        return Err(io::ErrorKind::InvalidInput.into());
                    }
                    let parent = destination.parent().ok_or(io::ErrorKind::InvalidInput)?;
                    let mut file = tempfile::NamedTempFile::new_in(parent)?;
                    file.as_file()
                        .set_permissions(fs::Permissions::from_mode(0o600))?;
                    file.write_all(&bytes)?;
                    file.as_file().sync_all()?;
                    if file.persist_noclobber(&destination).is_err() {
                        view("The destination already exists or could not be created. Choose a different name; no file was overwritten.","Restore copy",&cancel)?;
                        continue;
                    }
                    fs::File::open(parent)?.sync_all()?;
                    view(
                        "A separate restored copy was saved. The original is unchanged.",
                        "Restore copy",
                        &cancel,
                    )?;
                }
                "Choose another version" => break,
                _ => return Err(io::ErrorKind::InvalidInput.into()),
            }
        }
    }
}
fn main() {
    if run(&std::env::args().skip(1).collect::<Vec<_>>(), helper).is_err() {
        eprintln!("Backup file operation failed. Configure backups first and unlock authentication when requested; originals are unchanged.");
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_fixture_child() {
        if let Ok(path) = std::env::var("SEELE_TEST_RESTORE_PATH") {
            run(&[path], |args, cancel| {
                helper_command("run0", Path::new("/fixture/root-helper"), args, cancel)
            })
            .unwrap();
        }
    }
    #[test]
    fn ui_fixture() {
        let status = Command::new("python3")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/restore_ui.py"))
            .arg(std::env::current_exe().unwrap())
            .arg("--test-binary")
            .status()
            .unwrap();
        assert!(status.success());
    }
}
