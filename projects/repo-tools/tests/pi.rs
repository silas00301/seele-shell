use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};
fn fixture(root: &Path, body: &str) -> PathBuf {
    let shell = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|p| p.join("sh"))
        .find(|p| p.is_file())
        .unwrap();
    let file = root.join("jj");
    fs::write(&file, format!("#!{}\n{}", shell.display(), body)).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
    file
}
fn command(binary: &Path, mode: &str, root: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_seele-pi-jj"))
        .args([binary.as_os_str(), mode.as_ref()])
        .current_dir(root)
        .output()
        .unwrap()
}
#[test]
fn native_jj_metadata_preserves_fixed_argv_and_non_repository_state() {
    let root = tempfile::tempdir().unwrap();
    let binary = fixture(
        root.path(),
        "printf '%s\\n' \"$*\" >> calls\ncase \"$1\" in root) printf '/fixture\\n';; log) printf 'branch 🌸';; *) exit 2;; esac\n",
    );
    let output = command(&binary, "detect", root.path());
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!({"repository":true,"revision":"branch 🌸"})
    );
    let calls = fs::read_to_string(root.path().join("calls")).unwrap();
    assert!(calls.starts_with(
        "root\nlog --no-graph --no-pager --color=never -r @ -T if(self.local_bookmarks()"
    ));
    let binary = fixture(root.path(), "exit 1\n");
    let output = command(&binary, "detect", root.path());
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["repository"],
        false
    );
}
#[test]
fn native_jj_metadata_bounds_output_and_runtime_without_exposing_diagnostics() {
    let root = tempfile::tempdir().unwrap();
    let binary = fixture(
        root.path(),
        "while :; do printf 'untrusted private output'; done\n",
    );
    let now = Instant::now();
    let output = command(&binary, "revision", root.path());
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    assert!(now.elapsed() < Duration::from_secs(5));
    let binary = fixture(root.path(), "while :; do :; done\n");
    let now = Instant::now();
    let output = command(&binary, "revision", root.path());
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(now.elapsed() < Duration::from_secs(5));
    let output = command(Path::new("relative-jj"), "detect", root.path());
    assert!(!output.status.success());
}
