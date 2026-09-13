//! Runs the actual native entry point against private fake desktop commands.
//! No compositor, audio server, system profile or activation is contacted.
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};
struct Fixture {
    root: tempfile::TempDir,
    path: String,
}
impl Fixture {
    fn new() -> Self {
        let root = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let existing = std::env::var("PATH").unwrap();
        let shell = std::env::split_paths(&existing)
            .map(|p| p.join("sh"))
            .find(|p| p.is_file())
            .unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        for (name, script) in [
            (
                "hyprctl",
                r#"
if [ "$1" = clients ]; then
  printf '%s\n' '[{"address":"0x1","class":"ghostty","title":"One","focusHistoryID":2,"mapped":true,"hidden":false,"workspace":{"id":1,"name":"main"}},{"address":"0x2","class":"vicinae","title":"Picker","focusHistoryID":0,"mapped":true},{"address":"0x3","class":"ghostty","title":"Three","focusHistoryID":1,"mapped":true}]'
elif [ "$1" = binds ]; then
  printf '%s\n' '[{"key":"S","modmask":68,"description":"__lua 42"}]'
elif [ "$1" = workspaces ]; then
  printf '%s\n' '[{"id":2,"name":"two","monitor":"DP-1","windows":1},{"id":-1,"name":"hidden"},{"id":1,"name":"one","monitor":"DP-1","windows":2}]'
else
  printf '%s\n' "$@" >"$FIXTURE_ROOT/hyprctl-argv"
  if [ "$FIXTURE_MODE" = rejected ]; then printf 'private Lua error\n'; else printf 'ok\n'; fi
fi
"#,
            ),
            ("wtype", r#"printf '%s\n' "$@" >"$FIXTURE_ROOT/wtype-argv""#),
            (
                "nvd",
                r#"
printf '%s\n' "$@" >"$FIXTURE_ROOT/nvd-argv"
printf '\033[31m- old\033[0m\n+ new\n'
"#,
            ),
        ] {
            let file = bin.join(name);
            fs::write(&file, format!("#!{}\n{script}", shell.display())).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(0o700)).unwrap();
        }
        Self {
            path: format!("{}:{existing}", bin.display()),
            root,
        }
    }
    fn run(&self, args: &[&str], mode: &str) -> Output {
        Command::new(env!("CARGO_BIN_EXE_seele-tools"))
            .arg("control")
            .args(args)
            .env("PATH", &self.path)
            .env("FIXTURE_ROOT", self.root.path())
            .env("FIXTURE_MODE", mode)
            .env("HOME", self.root.path())
            .env("XDG_RUNTIME_DIR", self.root.path())
            .output()
            .unwrap()
    }
}
#[test]
fn snapshots_are_native_bounded_projections_and_exclude_launcher() {
    let f = Fixture::new();
    let result = f.run(&["vicinae-desktop"], "");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["clientGroups"][0][0]["address"], "0x3");
    assert_eq!(value["clientGroups"][1][0]["address"], "0x1");
    assert_eq!(value["clientGroups"].as_array().unwrap().len(), 2);
    assert_eq!(
        value["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|w| w["id"].clone())
            .collect::<Vec<_>>(),
        vec![json!(1), json!(2)]
    );
}
#[test]
fn invalid_focus_never_launches_and_zero_exit_lua_errors_fail() {
    let f = Fixture::new();
    for args in [
        vec!["vicinae-focus", "window", "0x1\" }); bad()"],
        vec!["vicinae-focus", "workspace", "-1"],
        vec!["vicinae-focus", "workspace", "1.5"],
        vec!["vicinae-focus", "workspace", "\"1\""],
        vec!["vicinae-focus", "workspace", "9007199254740992"],
    ] {
        assert!(!f.run(&args, "").status.success());
        assert!(!f.root.path().join("hyprctl-argv").exists());
    }
    let result = f.run(&["vicinae-focus", "window", "0xABC12"], "");
    assert!(result.status.success());
    assert_eq!(
        fs::read_to_string(f.root.path().join("hyprctl-argv")).unwrap(),
        "dispatch\nhl.dsp.focus({ window = \"address:0xabc12\" })\n"
    );
    let result = f.run(&["vicinae-focus", "workspace", "12"], "rejected");
    assert!(!result.status.success());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("private Lua error"));
}
#[test]
fn diff_uses_only_the_two_reviewed_immutable_closures() {
    let f = Fixture::new();
    let target = "00000000000000000000000000000000-target";
    let running = "11111111111111111111111111111111-running";
    let result = f.run(&["vicinae-generation-diff", "42", target, running], "");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&result.stdout).unwrap(),
        json!({"diff":"    - old\n    + new"})
    );
    assert_eq!(
        fs::read_to_string(f.root.path().join("nvd-argv")).unwrap(),
        format!("diff\n/nix/store/{running}\n/nix/store/{target}\n")
    );
    fs::remove_file(f.root.path().join("nvd-argv")).unwrap();
    assert!(!f
        .run(
            &["vicinae-generation-diff", "42", "../unreviewed", running],
            ""
        )
        .status
        .success());
    assert!(!f.root.path().join("nvd-argv").exists());
}

#[test]
fn keybinding_snapshot_and_input_use_the_native_validated_contract() {
    let f = Fixture::new();
    let output = f.run(&["vicinae-keybindings"], "");
    assert!(output.status.success());
    let rows: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["shortcut"], "Super + Ctrl + S");
    assert_eq!(
        rows[0]["description"],
        "Open a visible URI from the frozen screens"
    );
    for row in [r#"{"key":"x","modmask":-1}"#, r#"{"key":"\n"}"#] {
        assert!(!f
            .run(&["vicinae-input-keybinding", row], "")
            .status
            .success());
        assert!(!f.root.path().join("wtype-argv").exists());
    }
    assert!(f
        .run(
            &["vicinae-input-keybinding", r#"{"key":"S","modmask":68}"#],
            ""
        )
        .status
        .success());
    assert_eq!(
        fs::read_to_string(f.root.path().join("wtype-argv")).unwrap(),
        "-M\nlogo\n-M\nctrl\n-k\ns\n-m\nctrl\n-m\nlogo\n"
    );
}
