//! Narrow privileged NixOS generation activation. All paths are fixed by this
//! executable; caller environment and arbitrary executable arguments are unused.
use crate::Result;
use seele_runtime::nix::store_name;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;

struct Roots<'a> {
    profiles: &'a Path,
    store: &'a Path,
    running: &'a Path,
    owner: u32,
}

fn positive_generation(arguments: &[String]) -> Result<&str> {
    let value = arguments.first().ok_or(64)?;
    if arguments.len() != 3
        || value.is_empty()
        || value.len() > 20
        || value.starts_with('0')
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || value.parse::<u64>().is_err()
        || !store_name(&arguments[1])
        || !store_name(&arguments[2])
    {
        return Err(64);
    }
    Ok(value)
}

fn trusted(path: &Path, owner: u32, executable: bool) -> Result<()> {
    let metadata = fs::metadata(path).map_err(|_| 66)?;
    if metadata.uid() != owner
        || metadata.mode() & 0o022 != 0
        || if executable {
            !metadata.is_file() || metadata.mode() & 0o111 == 0
        } else {
            !metadata.is_dir()
        }
    {
        return Err(66);
    }
    Ok(())
}

fn closure(path: &Path, roots: &Roots<'_>) -> Result<PathBuf> {
    let resolved = fs::canonicalize(path).map_err(|_| 66)?;
    if resolved.parent() != Some(roots.store)
        || !resolved
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(store_name)
    {
        return Err(66);
    }
    trusted(&resolved, roots.owner, false)?;
    Ok(resolved)
}

fn executable(path: &Path, roots: &Roots<'_>) -> Result<PathBuf> {
    let resolved = fs::canonicalize(path).map_err(|_| 66)?;
    let relative = resolved.strip_prefix(roots.store).map_err(|_| 66)?;
    if relative.components().count() < 2 {
        return Err(66);
    }
    trusted(&resolved, roots.owner, true)?;
    let mut parent = resolved.parent();
    while let Some(directory) = parent.filter(|directory| *directory != roots.store) {
        trusted(directory, roots.owner, false)?;
        parent = directory.parent();
    }
    Ok(resolved)
}

fn switch(
    generation: &str,
    expected_target: &str,
    expected_running: &str,
    roots: Roots<'_>,
    mut run: impl FnMut(&Path, &[&std::ffi::OsStr]) -> Result<()>,
) -> Result<()> {
    trusted(roots.profiles, roots.owner, false)?;
    trusted(roots.store, roots.owner, false)?;
    let target_link = roots.profiles.join(format!("system-{generation}-link"));
    let target = closure(&target_link, &roots)?;
    let running = closure(roots.running, &roots).map_err(|_| 69)?;
    // Authentication can outlive the UI's last check. Caller identities are
    // comparison-only basenames; every executable still comes from fixed,
    // independently resolved system paths.
    if target.file_name() != Some(expected_target.as_ref())
        || running.file_name() != Some(expected_running.as_ref())
    {
        return Err(66);
    }
    let activation = executable(&target.join("bin/switch-to-configuration"), &roots)?;
    let nix_env = executable(&running.join("sw/bin/nix-env"), &roots).map_err(|_| 69)?;
    if target == running {
        return Ok(());
    }
    let profile = roots.profiles.join("system");
    // Recheck the selected link immediately before changing the system profile.
    if closure(&target_link, &roots)? != target || closure(roots.running, &roots)? != running {
        return Err(66);
    }
    run(
        &nix_env,
        &[
            "--profile".as_ref(),
            profile.as_os_str(),
            "--switch-generation".as_ref(),
            generation.as_ref(),
        ],
    )?;
    // Concurrent administrator changes must not activate a different closure
    // from the profile just selected. Never execute through the mutable link.
    if closure(&profile, &roots)? != target
        || closure(&target_link, &roots)? != target
        || closure(roots.running, &roots)? != running
    {
        return Err(66);
    }
    run(&activation, &["switch".as_ref()])
}

pub fn run(arguments: &[String], cancel: &AtomicUsize) -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(64);
    }
    let generation = positive_generation(arguments)?;
    switch(
        generation,
        &arguments[1],
        &arguments[2],
        Roots {
            profiles: Path::new("/nix/var/nix/profiles"),
            store: Path::new("/nix/store"),
            running: Path::new("/run/current-system"),
            owner: 0,
        },
        |program, arguments| {
            let mut command = Command::new(program);
            command
                .env_clear()
                .env("HOME", "/root")
                .env("PATH", "/run/current-system/sw/bin")
                .env("LANG", "C.UTF-8")
                .current_dir("/")
                .args(arguments);
            crate::interactive(&mut command, cancel)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use tempfile::TempDir;
    const TARGET: &str = "00000000000000000000000000000000-target-system";
    const RUNNING: &str = "11111111111111111111111111111111-running-system";
    struct Fixture {
        _temporary: TempDir,
        profiles: PathBuf,
        store: PathBuf,
        running: PathBuf,
        target: PathBuf,
        old: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let temporary = tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap();
            let profiles = temporary.path().join("profiles");
            let store = temporary.path().join("store");
            let running = temporary.path().join("running");
            let target = store.join(TARGET);
            let old = store.join(RUNNING);
            fs::create_dir(&profiles).unwrap();
            for (base, executable) in [
                (&target, "bin/switch-to-configuration"),
                (&old, "sw/bin/nix-env"),
            ] {
                let path = base.join(executable);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(&path, b"fixture only: never executed").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o500)).unwrap();
            }
            symlink(&target, profiles.join("system-42-link")).unwrap();
            symlink(&old, &running).unwrap();
            Self {
                _temporary: temporary,
                profiles,
                store,
                running,
                target,
                old,
            }
        }
        fn roots(&self) -> Roots<'_> {
            Roots {
                profiles: &self.profiles,
                store: &self.store,
                running: &self.running,
                owner: unsafe { libc::geteuid() },
            }
        }
    }
    #[test]
    fn strictly_validates_generation_and_comparison_only_store_identities() {
        for values in [
            vec![],
            vec!["0"],
            vec!["01"],
            vec!["-1"],
            vec!["1;reboot"],
            vec!["1", "2"],
            vec!["18446744073709551616"],
            vec!["+1"],
            vec![" 1"],
        ] {
            assert_eq!(
                positive_generation(&values.into_iter().map(str::to_owned).collect::<Vec<_>>()),
                Err(64)
            );
        }
        assert_eq!(
            positive_generation(&["42".into(), TARGET.into(), RUNNING.into()]),
            Ok("42")
        );
        for number in [
            "0",
            "01",
            "-1",
            "+1",
            "1;reboot",
            "18446744073709551616",
            " 1",
        ] {
            assert_eq!(
                positive_generation(&[number.into(), TARGET.into(), RUNNING.into()]),
                Err(64)
            );
        }
        for identity in [
            "",
            "target-system",
            "/nix/store/name",
            "../target",
            "00000000000000000000000000000000-../x",
            "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-bad-hash",
        ] {
            assert_eq!(
                positive_generation(&["42".into(), identity.into(), RUNNING.into()]),
                Err(64)
            );
            assert_eq!(
                positive_generation(&["42".into(), TARGET.into(), identity.into()]),
                Err(64)
            );
        }
    }
    #[test]
    fn uses_running_tools_then_activates_exact_validated_closure() {
        let fixture = Fixture::new();
        let mut calls = Vec::new();
        switch("42", TARGET, RUNNING, fixture.roots(), |program, args| {
            calls.push((
                program.to_owned(),
                args.iter().map(|s| s.to_os_string()).collect::<Vec<_>>(),
            ));
            if calls.len() == 1 {
                symlink(&fixture.target, fixture.profiles.join("system")).unwrap();
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(calls[0].0, fixture.old.join("sw/bin/nix-env"));
        assert_eq!(
            calls[0].1,
            [
                "--profile".as_ref(),
                fixture.profiles.join("system").as_os_str(),
                "--switch-generation".as_ref(),
                "42".as_ref()
            ]
        );
        assert_eq!(
            calls[1].0,
            fixture.target.join("bin/switch-to-configuration")
        );
        assert_eq!(calls[1].1, ["switch"]);
    }
    #[test]
    fn failures_or_profile_races_never_activate() {
        let fixture = Fixture::new();
        assert_eq!(
            switch("42", TARGET, RUNNING, fixture.roots(), |_, _| Err(17)),
            Err(17)
        );
        let mut calls = 0;
        assert_eq!(
            switch("42", TARGET, RUNNING, fixture.roots(), |_, _| {
                calls += 1;
                symlink(&fixture.old, fixture.profiles.join("system")).unwrap();
                Ok(())
            }),
            Err(66)
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn review_identities_are_rechecked_after_authorization() {
        let fixture = Fixture::new();
        let changed = "22222222222222222222222222222222-changed-system";
        for (target, running) in [(changed, RUNNING), (TARGET, changed)] {
            assert_eq!(
                switch("42", target, running, fixture.roots(), |_, _| panic!(
                    "must not run"
                )),
                Err(66)
            );
        }
        // A positive numeric generation was retained but now points elsewhere.
        fs::remove_file(fixture.profiles.join("system-42-link")).unwrap();
        symlink(&fixture.old, fixture.profiles.join("system-42-link")).unwrap();
        assert_eq!(
            switch("42", TARGET, RUNNING, fixture.roots(), |_, _| panic!(
                "must not run"
            )),
            Err(66)
        );
    }
    #[test]
    fn concurrent_running_system_change_never_activates() {
        let fixture = Fixture::new();
        let mut calls = 0;
        assert_eq!(
            switch("42", TARGET, RUNNING, fixture.roots(), |_, _| {
                calls += 1;
                symlink(&fixture.target, fixture.profiles.join("system")).unwrap();
                fs::remove_file(&fixture.running).unwrap();
                symlink(&fixture.target, &fixture.running).unwrap();
                Ok(())
            }),
            Err(66)
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn rejects_paths_outside_store_and_writable_executables() {
        let fixture = Fixture::new();
        fs::remove_file(fixture.profiles.join("system-42-link")).unwrap();
        symlink(&fixture.profiles, fixture.profiles.join("system-42-link")).unwrap();
        assert_eq!(
            switch("42", TARGET, RUNNING, fixture.roots(), |_, _| panic!(
                "must not run"
            )),
            Err(66)
        );
        fs::remove_file(fixture.profiles.join("system-42-link")).unwrap();
        symlink(&fixture.target, fixture.profiles.join("system-42-link")).unwrap();
        fs::set_permissions(
            fixture.target.join("bin/switch-to-configuration"),
            fs::Permissions::from_mode(0o777),
        )
        .unwrap();
        assert_eq!(
            switch("42", TARGET, RUNNING, fixture.roots(), |_, _| panic!(
                "must not run"
            )),
            Err(66)
        );
    }
}
