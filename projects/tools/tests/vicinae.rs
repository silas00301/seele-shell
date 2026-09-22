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
elif [ "$1" = workspacerules ]; then
  printf '%s\n' '[]'
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
            .env("FIXTURE_PID", std::process::id().to_string())
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
    assert_eq!(rows[0]["modifiers"], json!(["Super", "Ctrl"]));
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

impl Fixture {
    fn moving() -> Self {
        let f = Self::new();
        let python = std::env::split_paths(&std::env::var("PATH").unwrap())
            .map(|p| p.join("python3"))
            .find(|p| p.is_file())
            .unwrap();
        fs::write(f.root.path().join("bin/hyprctl"), format!("#!{}\n{}", python.display(), r#"
import json, os, sys
from pathlib import Path
root = Path(os.environ['FIXTURE_ROOT'])
mode = os.environ['FIXTURE_MODE']
op = sys.argv[1]
moved = (root / 'moved').exists()
with (root / 'queries').open('a') as log: log.write(op + '\n')
pid = int(os.environ['FIXTURE_PID'])
client = dict(address='0x1', pid=pid, initialClass='ghostty', initialTitle='Terminal',
              title='Current title', **{'class':'ghostty'}, mapped=True, hidden=False,
              focusHistoryID=1, workspace=dict(id=1, name='one'))
if mode == 'recycled': client['pid'] = 999
if mode == 'reclassified': client['initialClass'] = 'different'
if mode == 'retitled': client['initialTitle'] = 'different'
if mode == 'hidden': client['hidden'] = True
if mode == 'source-moved': client['workspace'] = dict(id=8, name='eight')
if mode == 'special-source': client['workspace'] = dict(id=-99, name='special:scratchpad')
if moved and mode not in ('noop', 'rejected'):
    target = json.loads((root / 'moved').read_text())
    client['workspace'] = dict(id=2, name='two') if target == '2' else (
        dict(id=9, name='9') if target == '9' else dict(id=-1337, name=target[5:]))
    if mode == 'recycled-after': client['pid'] = 999
if op == 'clients':
    print(json.dumps([] if mode == 'closed' else [client]))
elif op == 'workspaces':
    print(json.dumps([dict(id=1, name='one'), dict(id=-99, name='special:scratchpad')]
      + ([] if mode == 'target-gone' else [dict(id=2, name='renamed' if mode == 'target-renamed' else 'two')])
      + [dict(id=-1338, name='research')]))
elif op == 'workspacerules':
    if mode == 'rules-unavailable': sys.exit(1)
    if mode == 'rules-malformed': print('{}'); sys.exit(0)
    print(json.dumps([dict(workspaceString=s) for s in ['1', '2', '9', 'name:writing',
       'name:research', 'special:scratchpad', 'r[1-20]', '+1', '0', 'm+1']] +
       [dict(workspaceString='12', enabled=False)]))
elif op == 'activewindow':
    print(json.dumps(dict(address='0x1' if mode == 'active-source' and not moved else
        ('0x3' if mode == 'focus-stolen' and moved else '0x2'), pid=pid if mode == 'active-source' and not moved else 202)))
elif op == 'activeworkspace':
    print(json.dumps(dict(id=2 if mode == 'workspace-switched' and moved else 1)))
elif op == 'dispatch':
    (root / 'hyprctl-argv').write_text('\n'.join(sys.argv[1:]) + '\n')
    if mode == 'rejected': print('private Lua error')
    else:
        expression = sys.argv[2]
        target = json.loads(expression.split('workspace = ', 1)[1].split(', follow = false', 1)[0])
        (root / 'moved').write_text(json.dumps(target))
        print('ok')
else: sys.exit(1)
"#)).unwrap();
        f
    }
    fn selection(&self, selector: &str) -> Value {
        let output = self.run(&["vicinae-desktop"], "");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
        let client = &snapshot["clientGroups"][0][0];
        let destination = client["moveTargets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["selector"] == selector)
            .unwrap();
        json!({"window":client["moveWindow"],"destination":destination})
    }
}
#[test]
fn move_destinations_are_compositor_owned_and_exclude_current_special_and_patterns() {
    let f = Fixture::moving();
    let output = f.run(&["vicinae-desktop"], "");
    assert!(output.status.success());
    let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
    let client = &snapshot["clientGroups"][0][0];
    assert_eq!(client["moveWindow"]["pid"], std::process::id());
    assert_eq!(
        client["moveTargets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["selector"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["2", "9", "name:research", "name:writing"]
    );
    let output = f.run(&["vicinae-desktop"], "special-source");
    let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(snapshot["clientGroups"][0][0]["moveTargets"], json!([]));
}
#[test]
fn silent_moves_use_pinned_lua_api_and_verify_existing_configured_and_named_destinations() {
    for selector in ["2", "9", "name:writing"] {
        let f = Fixture::moving();
        let request = f.selection(selector).to_string();
        let output = f.run(&["vicinae-window-move", &request], "");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(fs::read_to_string(f.root.path().join("hyprctl-argv")).unwrap(),
            format!("dispatch\nhl.dsp.window.move({{ window = \"address:0x1\", workspace = \"{selector}\", follow = false }})\n"));
        assert!(fs::read_to_string(f.root.path().join("queries"))
            .unwrap()
            .ends_with("clients\nactivewindow\nactiveworkspace\n"));
    }
    let f = Fixture::moving();
    let request = f.selection("2").to_string();
    assert!(f
        .run(&["vicinae-window-move", &request], "active-source")
        .status
        .success());
}
#[test]
fn stale_windows_and_destinations_never_dispatch() {
    for mode in [
        "recycled",
        "reclassified",
        "retitled",
        "hidden",
        "source-moved",
        "special-source",
        "closed",
        "target-gone",
        "target-renamed",
    ] {
        let f = Fixture::moving();
        let request = f.selection("2").to_string();
        assert!(
            !f.run(&["vicinae-window-move", &request], mode)
                .status
                .success(),
            "{mode}"
        );
        assert!(!f.root.path().join("hyprctl-argv").exists(), "{mode}");
    }
}
#[test]
fn successful_exit_is_not_proof_of_a_silent_move() {
    for mode in [
        "rejected",
        "noop",
        "recycled-after",
        "focus-stolen",
        "workspace-switched",
    ] {
        let f = Fixture::moving();
        let request = f.selection("2").to_string();
        let output = f.run(&["vicinae-window-move", &request], mode);
        assert!(!output.status.success(), "{mode}");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private Lua error"));
    }
}
#[test]
fn malformed_moves_and_injected_selectors_never_dispatch() {
    let f = Fixture::moving();
    for payload in ["{}", "[]", r#"{"window":null,"destination":null}"#] {
        assert!(!f
            .run(&["vicinae-window-move", payload], "")
            .status
            .success());
    }
    for selector in [
        "special:scratchpad",
        "r[1-20]",
        "-99",
        "2\n",
        "name:a\n",
        "999999999999999999999",
        "2\" }); bad()",
    ] {
        let mut request = f.selection("2");
        request["destination"]["selector"] = json!(selector);
        assert!(!f
            .run(&["vicinae-window-move", &request.to_string()], "")
            .status
            .success());
        assert!(!f.root.path().join("hyprctl-argv").exists());
    }
}

#[test]
fn unavailable_rules_keep_live_windows_and_destinations_available() {
    for mode in ["rules-unavailable", "rules-malformed"] {
        let f = Fixture::moving();
        let output = f.run(&["vicinae-desktop"], mode);
        assert!(output.status.success());
        let snapshot: Value = serde_json::from_slice(&output.stdout).unwrap();
        let client = &snapshot["clientGroups"][0][0];
        assert_eq!(client["moveTargets"].as_array().unwrap().len(), 2);
        assert!(!client["moveWindow"].to_string().contains("Terminal"));
        let request = f.selection("2").to_string();
        assert!(f
            .run(&["vicinae-window-move", &request], mode)
            .status
            .success());
    }
}

#[test]
fn recycled_process_start_time_is_rejected() {
    let f = Fixture::moving();
    let mut request = f.selection("2");
    request["window"]["started"] = json!(0);
    assert!(!f
        .run(&["vicinae-window-move", &request.to_string()], "")
        .status
        .success());
    assert!(!f.root.path().join("hyprctl-argv").exists());
}
