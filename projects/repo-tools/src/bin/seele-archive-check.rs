use std::{
    fs::{self, OpenOptions},
    io::{Read, Seek},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt},
        process::CommandExt,
    },
    path::PathBuf,
    process::Command,
    time::Duration,
};
fn run() -> Result<(), &'static str> {
    let mut args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|arg| arg == "--") {
        args.remove(0);
    }
    if args.len() != 1 {
        return Err("Usage: seele-archive-check -- ARCHIVE");
    }
    let path = PathBuf::from(&args[0]);
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&path)
        .map_err(|_| "Cannot open a regular archive (symlinks are refused).")?;
    let before = file.metadata().map_err(|_| "Cannot inspect archive.")?;
    if !before.is_file() || before.len() > 4 * 1024 * 1024 * 1024 {
        return Err("Choose a regular archive no larger than 4 GiB.");
    }
    let mut header = [0u8; 512];
    let count = file
        .read(&mut header)
        .map_err(|_| "Cannot read archive header.")?;
    let format = format(&header[..count])
        .ok_or("Supported containers: ZIP, 7z, RAR, gzip, xz, bzip2 and ustar TAR.")?;
    file.rewind().map_err(|_| "Cannot seek archive.")?;
    let cancel = seele_runtime::process::termination_signal()
        .map_err(|_| "Cannot establish cancellation.")?;
    let mut command = Command::new("7zz");
    command.args([
        "t",
        "-y",
        "-bd",
        "-bso0",
        "-bse0",
        "-bsp0",
        "-mmt=2",
        &format!("-t{format}"),
    ]);
    let fd = seele_runtime::process::inherit_file(&mut command, &file)
        .map_err(|_| "Cannot hand off archive descriptor.")?;
    let name = if cfg!(target_os = "macos") {
        format!("/dev/fd/{fd}")
    } else {
        format!("/proc/self/fd/{fd}")
    };
    command.args(["--", &name]);
    unsafe {
        command.pre_exec(|| {
            let memory = libc::rlimit {
                rlim_cur: 512 * 1024 * 1024,
                rlim_max: 512 * 1024 * 1024,
            };
            let cpu = libc::rlimit {
                rlim_cur: 60,
                rlim_max: 60,
            };
            if libc::setrlimit(libc::RLIMIT_AS, &memory) != 0
                || libc::setrlimit(libc::RLIMIT_CPU, &cpu) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    // Closed stdin means encrypted archives cannot ask for a password. There
    // is no extraction command, destination directory or writable file handle.
    let output = seele_runtime::process::capture(
        &mut command,
        b"",
        seele_runtime::process::Limits {
            timeout: Duration::from_secs(120),
            output: 64 * 1024,
        },
        &cancel,
    )
    .map_err(|_| "Archive check cancelled, timed out or exceeded its bounds.")?;
    if !output.status.success() {
        return Err(
            "Integrity check failed: corrupt, encrypted, unsupported or too resource-intensive.",
        );
    }
    let after = file.metadata().map_err(|_| "Cannot recheck archive.")?;
    let named = fs::symlink_metadata(&path).map_err(|_| "Archive moved during check.")?;
    for current in [&after, &named] {
        if before.dev() != current.dev()
            || before.ino() != current.ino()
            || before.len() != current.len()
            || before.mtime() != current.mtime()
            || before.mtime_nsec() != current.mtime_nsec()
            || before.ctime() != current.ctime()
            || before.ctime_nsec() != current.ctime_nsec()
        {
            return Err("Archive changed during check; retry after its writer finishes.");
        }
    }
    println!("Archive integrity check passed (read-only).");
    Ok(())
}
fn format(header: &[u8]) -> Option<&'static str> {
    if header.starts_with(b"PK\x03\x04") || header.starts_with(b"PK\x05\x06") {
        Some("zip")
    } else if header.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Some("7z")
    } else if header.starts_with(b"Rar!\x1a\x07") {
        Some("rar")
    } else if header.starts_with(b"\x1f\x8b") {
        Some("gzip")
    } else if header.starts_with(b"\xfd7zXZ\0") {
        Some("xz")
    } else if header.starts_with(b"BZh") {
        Some("bzip2")
    } else if header.get(257..262) == Some(b"ustar") {
        Some("tar")
    } else {
        None
    }
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn magic_not_suffix() {
        assert_eq!(format(b"PK\x03\x04"), Some("zip"));
        assert_eq!(format(b"\x1f\x8b"), Some("gzip"));
        assert_eq!(format(b"not zip"), None);
    }
}
