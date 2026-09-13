use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn program(name: &str) -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join(name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| panic!("fixture needs {name} on PATH"))
}

#[test]
fn portable_launch_materializes_then_execs_with_literal_arguments_and_environment() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let source = root.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("settings"), "fixture settings").unwrap();
    let target = root.join("application");
    fs::write(
        &target,
        format!(
        "#!{}\nprintf '%s\\n' \"$$\" \"$XDG_CONFIG_HOME\" \"$PATH\" \"$LITERAL\" \"$@\"\nexit 17\n",
        program("sh").display()
    ),
    )
    .unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();
    let manifest = root.join("launch.json");
    fs::write(&manifest, serde_json::to_vec(&json!({
        "version":1,"program":target,"arguments":["fixed argument"],
        "path":["/declared/bin"],
        "environment":{
            "LITERAL":"${INHERITED}",
            "XDG_CONFIG_HOME":"${SEELE_PORTABLE_HOME:-${XDG_CACHE_HOME:-$HOME/.cache}/seele/portable}/app"
        },
        "configuration":{"source":source,"destination":"$HOME/.cache/seele/portable/app"}
    })).unwrap()).unwrap();
    let child = Command::new(env!("CARGO_BIN_EXE_seele-launch"))
        .arg(&manifest)
        .arg("a b")
        .arg("'$(false)' * 🦀")
        .env_clear()
        .env("HOME", root)
        .env("PATH", "/original/bin")
        .env("INHERITED", "$(false) `false` *.md")
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(17));
    let destination = root.join(".cache/seele/portable/app");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!(
            "{pid}\n{}\n/declared/bin:/original/bin\n$(false) `false` *.md\nfixed argument\na b\n'$(false)' * 🦀\n",
            destination.display()
        )
    );
    assert_eq!(
        fs::read_to_string(destination.join("settings")).unwrap(),
        "fixture settings"
    );
    assert!(fs::symlink_metadata(destination.join("settings"))
        .unwrap()
        .is_symlink());
}

#[test]
fn executable_templates_cannot_run_commands_or_accept_unknown_fields() {
    let temporary = tempfile::tempdir().unwrap();
    let manifest = temporary.path().join("launch.json");
    for value in [
        json!({"version":1,"program":program("sh"),"environment":{"HOME":"$(exit 0)"}}),
        json!({"version":1,"program":program("sh"),"extra":"not permitted"}),
        json!({"version":1,"program":"relative-program"}),
        json!({"version":2,"program":program("sh")}),
    ] {
        fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_seele-launch"))
            .arg(&manifest)
            .env_clear()
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
}

#[test]
fn numbered_home_backup_preserves_existing_backups_and_dash_prefixed_names() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    fs::write(root.join("-configuration"), "new").unwrap();
    fs::write(root.join("-configuration.bak"), "older").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_seele-home-backup"))
        .arg("-configuration")
        .current_dir(root)
        .env_clear()
        .env("SEELE_MV", program("mv"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(root.join("-configuration.bak")).unwrap(),
        "new"
    );
    assert_eq!(
        fs::read_to_string(root.join("-configuration.bak.~1~")).unwrap(),
        "older"
    );
    assert!(!root.join("-configuration").exists());
}

#[test]
fn invalid_launch_declarations_never_materialize_configuration() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let source = root.join("source");
    let destination = root.join("destination");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("settings"), "private fixture").unwrap();
    let manifest = root.join("launch.json");
    for invalid in [
        json!({"environment":{"BROKEN":"$(false)"}}),
        json!({"environment":{"INVALID-NAME":"data"}}),
        json!({"path":["/two:/paths"]}),
        json!({"arguments":["nul\u{0000}argument"]}),
    ] {
        let mut value = json!({"version":1,"program":program("sh"),
            "configuration":{"source":source,"destination":destination}});
        value
            .as_object_mut()
            .unwrap()
            .extend(invalid.as_object().unwrap().clone());
        fs::write(&manifest, serde_json::to_vec(&value).unwrap()).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_seele-launch"))
            .arg(&manifest)
            .env_clear()
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(!destination.exists(), "invalid declaration published files");
        assert!(!root.join(".destination.lock").exists());
    }
}

#[test]
fn backups_use_exact_destination_for_directories_and_directory_symlinks() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let backup = |name| {
        Command::new(env!("CARGO_BIN_EXE_seele-home-backup"))
            .arg(name)
            .current_dir(root)
            .env_clear()
            .env("SEELE_MV", program("mv"))
            .output()
            .unwrap()
    };
    fs::create_dir(root.join("directory")).unwrap();
    fs::write(root.join("directory/new"), "new").unwrap();
    fs::create_dir(root.join("directory.bak")).unwrap();
    fs::write(root.join("directory.bak/old"), "old").unwrap();
    assert!(backup("directory").status.success());
    assert_eq!(
        fs::read_to_string(root.join("directory.bak/new")).unwrap(),
        "new"
    );
    assert_eq!(
        fs::read_to_string(root.join("directory.bak.~1~/old")).unwrap(),
        "old"
    );
    fs::write(root.join("file"), "file").unwrap();
    std::os::unix::fs::symlink(root.join("directory.bak"), root.join("file.bak")).unwrap();
    assert!(backup("file").status.success());
    assert_eq!(fs::read_to_string(root.join("file.bak")).unwrap(), "file");
    assert!(fs::symlink_metadata(root.join("file.bak.~1~"))
        .unwrap()
        .is_symlink());
    assert!(!root.join("directory.bak/file").exists());
}
