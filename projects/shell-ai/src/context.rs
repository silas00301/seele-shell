//! Bounded metadata only: no environment dump, history, file contents or tools.
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{CStr, CString},
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
const PRIORITY: [&str; 18] = [
    "jj",
    "nix",
    "nh",
    "fish",
    "direnv",
    "devenv",
    "rg",
    "fd",
    "fzf",
    "eza",
    "bat",
    "jq",
    "gh",
    "nvim",
    "systemctl",
    "journalctl",
    "run0",
    "sudo",
];
fn hidden(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    (name.starts_with(".env") && name != ".envrc")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || seele_runtime::redact::secret_name(&name)
}
fn name(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_control() || !seele_runtime::redact::visible(c) {
                '�'
            } else {
                c
            }
        })
        .take(160)
        .collect()
}
pub fn directory_listing(directory: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(directory) else {
        return vec![];
    };
    let deadline = Instant::now() + Duration::from_millis(75);
    let mut names = BTreeSet::new();
    for entry in entries.take(16384) {
        if Instant::now() > deadline {
            break;
        }
        let Ok(entry) = entry else { continue };
        let raw = entry.file_name();
        let raw = raw.to_string_lossy();
        if hidden(&raw) {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let mut label = name(&raw);
        if kind.is_dir() {
            label.push('/');
        } else if kind.is_symlink() {
            label.push('@');
        }
        names.insert((!kind.is_dir(), label.to_lowercase(), label));
        if names.len() > 32 {
            names.pop_last();
        }
    }
    names.into_iter().map(|(_, _, value)| value).collect()
}
pub fn available_commands(path: &str) -> Value {
    let paths: Vec<_> = path
        .split(':')
        .filter(|p| !p.is_empty())
        .take(64)
        .map(PathBuf::from)
        .collect();
    let count = AtomicUsize::new(0);
    let deadline = Instant::now() + Duration::from_millis(150);
    let commands: BTreeSet<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|worker| {
                let paths = &paths;
                let count = &count;
                scope.spawn(move || {
                    let mut names = BTreeSet::new();
                    for directory in paths.iter().skip(worker).step_by(4) {
                        let Ok(entries) = fs::read_dir(directory) else {
                            continue;
                        };
                        for entry in entries {
                            if Instant::now() > deadline
                                || count.fetch_add(1, Ordering::Relaxed) >= 16384
                            {
                                return names;
                            }
                            let Ok(entry) = entry else { continue };
                            let raw = entry.file_name();
                            let Some(name) = raw.to_str() else { continue };
                            if name.len() > 80
                                || name
                                    .chars()
                                    .any(|c| c.is_control() || !seele_runtime::redact::visible(c))
                            {
                                continue;
                            }
                            let Ok(metadata) = fs::metadata(entry.path()) else {
                                continue;
                            };
                            if !metadata.is_file() || metadata.mode() & 0o111 == 0 {
                                continue;
                            }
                            let Ok(path) = CString::new(entry.path().as_os_str().as_bytes()) else {
                                continue;
                            };
                            if unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0 {
                                names.insert(name.to_owned());
                            }
                        }
                    }
                    names
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let mut selected: Vec<_> = PRIORITY
        .iter()
        .filter(|p| commands.contains(**p))
        .map(|p| (*p).to_owned())
        .collect();
    let mut rest: Vec<_> = commands
        .iter()
        .filter(|p| !PRIORITY.contains(&p.as_str()))
        .cloned()
        .collect();
    rest.sort_by_key(|s| s.to_lowercase());
    let slots = 240 - selected.len();
    if rest.len() > slots {
        selected.extend((0..slots).map(|i| rest[i * (rest.len() - 1) / (slots - 1)].clone()));
    } else {
        selected.extend(rest);
    }
    json!({"count":commands.len(),"sample":selected})
}
pub fn repository_kind(directory: &Path) -> &'static str {
    for current in directory.ancestors().take(256) {
        if current.join(".jj").exists() {
            return "jujutsu";
        }
        if current.join(".git").exists() {
            return "git";
        }
    }
    "none"
}
fn os_release() -> Value {
    let mut result = BTreeMap::new();
    if let Ok(file) = fs::File::open("/etc/os-release") {
        use std::io::Read;
        let mut text = String::new();
        if file.take(16384).read_to_string(&mut text).is_ok() {
            for line in text.lines() {
                if let Some((key, value)) = line.split_once('=') {
                    if ["ID", "NAME", "VERSION_ID"].contains(&key) {
                        result.insert(
                            key.to_lowercase(),
                            crate::clean_display(value.trim().trim_matches('"'), 160),
                        );
                    }
                }
            }
        }
    }
    json!(result)
}
pub fn collect(directory: &Path, environment: &BTreeMap<String, String>) -> Value {
    let directory = directory
        .canonicalize()
        .unwrap_or_else(|_| directory.into());
    let mut uname: libc::utsname = unsafe { std::mem::zeroed() };
    let success = unsafe { libc::uname(&mut uname) } == 0;
    let (kernel, architecture) = if success {
        unsafe {
            (
                CStr::from_ptr(uname.sysname.as_ptr())
                    .to_string_lossy()
                    .into_owned(),
                CStr::from_ptr(uname.machine.as_ptr())
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    } else {
        (String::new(), String::new())
    };
    let nix = environment
        .get("IN_NIX_SHELL")
        .map(String::as_str)
        .unwrap_or("");
    json!({"platform":{"kernel":kernel,"architecture":architecture,"os_release":os_release(),"configuration":"NixOS with declarative package management","interactive_shell":"fish"},
        "working_directory":directory,"directory_listing":directory_listing(&directory),"repository":repository_kind(&directory),"available_commands":available_commands(environment.get("PATH").map(String::as_str).unwrap_or("")),
        "active_dev_shell":{"nix_shell":if ["pure","impure"].contains(&nix){json!(nix)}else{json!(!nix.is_empty())},"direnv":environment.get("DIRENV_DIR").is_some_and(|s|!s.is_empty()),"devenv":environment.get("DEVENV_ROOT").is_some_and(|s|!s.is_empty())},
        "version_control_preference":"Use Jujutsu for daily work; use Git only for interoperability."})
}
pub fn current() -> Value {
    let environment = ["PATH", "IN_NIX_SHELL", "DIRENV_DIR", "DEVENV_ROOT"]
        .into_iter()
        .filter_map(|key| std::env::var(key).ok().map(|v| (key.into(), v)))
        .collect();
    collect(
        &std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        &environment,
    )
}
