use seele_shell_ai::{
    capture::{capture_eligible, clean_stderr, CaptureState, Failure},
    context, suggestions, MAX_CAPTURE_BYTES, MAX_STDERR_BYTES,
};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};
#[test]
fn context_collects_bounded_metadata_without_contents_or_environment_secrets() {
    let root = tempfile::tempdir().unwrap();
    let bin = root.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let shell = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|path| path.join("sh"))
        .find(|path| path.is_file())
        .unwrap();
    fs::write(bin.join("jj"), format!("#!{}\nexit 0\n", shell.display())).unwrap();
    fs::set_permissions(bin.join("jj"), fs::Permissions::from_mode(0o755)).unwrap();
    symlink(bin.join("jj"), bin.join("nix")).unwrap();
    fs::write(bin.join("not-executable"), "no").unwrap();
    fs::write(
        root.path().join("visible.txt"),
        "VISIBLE_CONTENTS_MUST_NOT_LEAK",
    )
    .unwrap();
    fs::write(root.path().join(".env"), "API_TOKEN=TOP_SECRET").unwrap();
    fs::write(root.path().join("credentials.json"), "TOP_SECRET").unwrap();
    fs::create_dir(root.path().join(".jj")).unwrap();
    let environment = BTreeMap::from([
        ("PATH".into(), bin.to_str().unwrap().into()),
        ("IN_NIX_SHELL".into(), "pure".into()),
        ("DIRENV_DIR".into(), "/private/value".into()),
        ("API_TOKEN".into(), "TOP_SECRET".into()),
    ]);
    let result = context::collect(root.path(), &environment);
    assert_eq!(result["repository"], "jujutsu");
    assert!(result["directory_listing"]
        .as_array()
        .unwrap()
        .contains(&json!("visible.txt")));
    assert_eq!(result["available_commands"]["sample"], json!(["jj", "nix"]));
    assert_eq!(result["active_dev_shell"]["nix_shell"], "pure");
    assert_eq!(result["active_dev_shell"]["direnv"], true);
    for secret in [
        "TOP_SECRET",
        "VISIBLE_CONTENTS_MUST_NOT_LEAK",
        "/private/value",
        "credentials.json",
        ".env\"",
    ] {
        assert!(!result.to_string().contains(secret));
    }
    assert_eq!(
        context::collect(root.path(), &BTreeMap::new())["available_commands"]["sample"],
        json!([])
    );
}
#[test]
fn listing_and_command_samples_stay_bounded() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..400 {
        let path = root.path().join(format!("tool{index:04}"));
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(context::directory_listing(root.path()).len(), 32);
    let commands = context::available_commands(root.path().to_str().unwrap());
    assert_eq!(commands["sample"].as_array().unwrap().len(), 240);
    assert_eq!(commands["count"], 400);
}
#[test]
fn strict_suggestions_reject_controls_types_and_oversized_payloads() {
    let valid = json!({"suggestions":[{"command":"jj status","description":"Inspect the checkout","destructive":false}]});
    assert_eq!(suggestions::parse(&valid).unwrap()[0].command, "jj status");
    for command in [
        "printf one\nprintf two",
        "jj\tstatus",
        "jj status\u{202e}status",
        "\u{200d}jj status",
        "jj status\u{2028}more",
    ] {
        let mut invalid = valid.clone();
        invalid["suggestions"][0]["command"] = json!(command);
        assert!(suggestions::parse(&invalid).is_err());
    }
    let mut invalid = valid.clone();
    invalid["suggestions"][0]["command"] = json!("x".repeat(4097));
    assert!(suggestions::parse(&invalid).is_err());
    invalid = valid.clone();
    invalid["suggestions"][0]["destructive"] = json!("false");
    assert!(suggestions::parse(&invalid).is_err());
    assert!(suggestions::build_request("how", &"x".repeat(8193), json!({}), None).is_err());
    assert!(suggestions::build_request("how", "", json!({}), None).is_err());
}
#[test]
fn local_guard_overrides_model_label_and_catches_quoted_absolute_commands() {
    for command in [
        "blkdiscard /dev/sdb",
        "cp image.raw /dev/sdb",
        "cryptsetup luksFormat /dev/sdb1",
        "rm -rf result",
        "find . -type f -delete",
        "nix profile wipe-history --older-than 30d",
        "mkfs.ext4 /dev/sdb1",
        "dd if=image of=/dev/nvme0n1",
        "nix-collect-garbage -d",
        "nix-env --delete-generations old",
        "nix store gc",
        "nh clean all",
        "git reset --hard HEAD",
        "jj abandon @",
        "/bin/rm old",
        "'rm' old",
        "r\"\"m old",
        "r\\m old",
        "echo (rm old)",
        "bash -c 'unknown program'",
        "eval untrusted",
    ] {
        assert!(suggestions::is_destructive(command), "{command}");
    }
    for command in [
        "jj status",
        "nix flake show",
        "rg --files",
        "systemctl status sshd",
    ] {
        assert!(!suggestions::is_destructive(command), "{command}");
    }
    let parsed=suggestions::parse(&json!({"suggestions":[{"command":"rm old.log","description":"remove it","destructive":false}]})).unwrap();
    assert!(parsed[0].destructive);
    assert!(suggestions::format_insertion(&parsed[0]).starts_with("# DESTRUCTIVE"));
}
#[test]
fn memory_ring_keeps_only_last_failure_and_cleans_terminal_sequences() {
    let mut state = CaptureState::default();
    state.begin();
    state.append(b"\x1b[31mfirst failure\x1b[0m\r\n");
    state.finish("false", 1);
    let first = state.failure().unwrap();
    assert_eq!(
        first,
        Failure {
            command: "false".into(),
            status: 1,
            stderr: "first failure".into()
        }
    );
    state.begin();
    state.append(b"successful noise\n");
    state.finish("true", 0);
    assert_eq!(state.failure().unwrap(), first);
    state.begin();
    state.append(&vec![b'x'; MAX_CAPTURE_BYTES * 2]);
    state.append(b"second failure\n");
    state.finish("broken --again", 23);
    let last = state.failure().unwrap();
    assert_eq!(last.status, 23);
    assert!(last.stderr.ends_with("second failure"));
    assert!(last.stderr.len() <= MAX_STDERR_BYTES);
    assert_eq!(
        clean_stderr(b"\x1b]0;window title\x07plain\x1b[0m\n"),
        "plain"
    );
}
#[test]
fn explicit_debug_redacts_failures_and_preserves_bounded_context() {
    let failure = Failure {
        command: "command --token cli-secret".into(),
        status: 23,
        stderr: "password=server-secret\nservice failed".into(),
    };
    let request = suggestions::build_request(
        "debug",
        "fix it",
        json!({"repository":"jujutsu"}),
        Some(failure),
    )
    .unwrap();
    assert_eq!(request["consumer"], "shell-ai");
    assert_eq!(request["context"]["last_failure"]["exit_code"], 23);
    assert!(request["context"]["last_failure"]["stderr"]
        .as_str()
        .unwrap()
        .contains("service failed"));
    assert!(!request.to_string().contains("cli-secret"));
    assert!(!request.to_string().contains("server-secret"));
    assert_eq!(
        request["output"]["schema"]["properties"]["suggestions"]["maxItems"],
        4
    );
    assert!(request.get("tools").is_none());
    assert!(suggestions::build_request("debug", "", json!({}), None).is_err());
}
#[test]
fn only_plain_interactive_fish_is_wrapped() {
    let args = |s: &[&str]| s.iter().map(|s| (*s).into()).collect::<Vec<_>>();
    assert!(capture_eligible(
        &args(&["/nix/store/example/bin/fish"]),
        true,
        true
    ));
    assert!(capture_eligible(&args(&["-fish", "-il"]), true, true));
    for values in [
        vec!["fish", "-ic", "echo test"],
        vec!["fish", "script.fish"],
        vec!["bash"],
    ] {
        assert!(!capture_eligible(&args(&values), true, true));
    }
    assert!(!capture_eligible(&args(&["fish"]), false, true));
}
#[test]
fn unchanged_fish_binding_still_has_one_review_path() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../modules/features/programs/shell-ai.nix");
    let Ok(source) = fs::read_to_string(path) else {
        return;
    };
    assert!(source.contains("__seele_ai_insert how"));
    assert!(source.contains("__seele_ai_insert debug"));
    assert!(source.contains("commandline --replace -- \"$suggestion\""));
    assert_eq!(source.matches("return \"$command_status\"").count(), 2);
    assert!(!source.contains("commandline -f execute\n              return"));
}

#[test]
fn fzf_theme_is_preserved_without_executable_bindings() {
    assert_eq!(suggestions::fzf_style("--color='bg:#1e1e2e,fg:#cdd6f4' --bind='start:execute(evil)' --border=rounded --preview=evil --no-color"), vec!["--color=bg:#1e1e2e,fg:#cdd6f4", "--border=rounded", "--no-color"]);
}
