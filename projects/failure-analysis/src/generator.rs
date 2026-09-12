use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};
use std::{fs, io};
pub const DROP_IN: &str = "[Unit]\nOnFailure=seele-failure-report@%n.service\n";
pub fn services(directories: &[PathBuf]) -> BTreeSet<String> {
    let mut seen = HashSet::new();
    let mut services = BTreeSet::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok).take(65536) {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if !name.ends_with(".service")
                || !seen.insert(name.clone())
                || name.starts_with("seele-failure-")
            {
                continue;
            }
            if !crate::collect::valid_unit(&name) {
                continue;
            }
            let Ok(metadata) = entry.path().symlink_metadata() else {
                continue;
            };
            if metadata.file_type().is_symlink()
                && fs::canonicalize(entry.path()).is_ok_and(|p| p == Path::new("/dev/null"))
            {
                continue;
            }
            if metadata.is_file() || metadata.file_type().is_symlink() {
                services.insert(name);
            }
        }
    }
    services
}
pub fn generate(output: &Path) -> io::Result<()> {
    let directories = if let Some(value) = std::env::var_os("SEELE_FAILURE_UNIT_PATH") {
        std::env::split_paths(&value)
            .filter(|p| !p.as_os_str().is_empty())
            .collect::<Vec<_>>()
    } else {
        [
            "/etc/systemd/system",
            "/run/systemd/system",
            "/run/current-system/systemd/lib/systemd/system",
            "/usr/local/lib/systemd/system",
            "/usr/lib/systemd/system",
            "/lib/systemd/system",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect()
    };
    seele_runtime::fs::private_directory(output)?;
    for name in services(&directories) {
        seele_runtime::fs::atomic_write(
            &output
                .join(format!("{name}.d"))
                .join("50-seele-failure-report.conf"),
            DROP_IN.as_bytes(),
        )?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    #[test]
    fn precedence_masks_and_recursive_exclusion() {
        let root = tempfile::tempdir().unwrap();
        let high = root.path().join("high");
        let low = root.path().join("low");
        fs::create_dir(&high).unwrap();
        fs::create_dir(&low).unwrap();
        for name in [
            "alpha.service",
            "masked.service",
            "seele-failure-report@.service",
        ] {
            fs::write(low.join(name), "").unwrap();
        }
        fs::write(high.join("beta.service"), "").unwrap();
        symlink("/dev/null", high.join("masked.service")).unwrap();
        assert_eq!(
            services(&[high, low]),
            BTreeSet::from(["alpha.service".into(), "beta.service".into()])
        );
    }
}
