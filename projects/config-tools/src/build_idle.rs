//! One job class, `nix build`, and the two things this slice does with it.
//!
//! A scheduled theme step looks at the live process list and waits while any
//! such job is running. An unreadable list waits too: publishing a theme
//! during a build is worse than holding one that could have gone ahead. The
//! Fish hook asks `seele-build-idle notify` after a foreground command; that
//! path sends one ordinary notification when the command was a `nix build`
//! and its terminal window is not focused.

use seele_runtime::process::{capture, Limits};
use serde_json::Value;
use std::{
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

const MAX_SCAN: usize = 8192;
const MAX_COMMAND: usize = 4096;
const MAX_ANCESTORS: usize = 32;

pub struct Notice {
    pub title: &'static str,
    pub body: String,
}

/// `Some(true)` a build is running, `Some(false)` the scan finished without
/// one, `None` the list could not be read. `None` is not idle.
pub fn nix_build_running(proc_root: &Path) -> Option<bool> {
    let entries = fs::read_dir(proc_root).ok()?;
    let mut seen = 0usize;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        if pid == 0 {
            continue;
        }
        seen += 1;
        if seen > MAX_SCAN {
            return Some(true);
        }
        let dir = entry.path();
        match cmdline(&dir.join("cmdline")) {
            Some(args) if is_nix_build(&args) => return Some(true),
            Some(args) if !args.is_empty() => {}
            _ if comm_is_nix(&dir.join("comm")) => return Some(true),
            _ => {}
        }
    }
    Some(false)
}

/// argv of a real `nix build` or the legacy `nix-build` program. Flags before
/// the subcommand are missed on purpose: a false idle is the worse mistake.
pub fn is_nix_build(args: &[String]) -> bool {
    let Some(program) = args.first().map(|value| program_name(value)) else {
        return false;
    };
    program == "nix-build"
        || (program == "nix" && args.get(1).is_some_and(|value| value == "build"))
}

/// A single foreground command that is itself `nix build` or `nix-build`.
/// Compound commands, background jobs and anything that would have to be
/// evaluated are refused, so a line that merely mentions the words does not
/// notify, and a pipeline's status is not reported as the build's.
pub fn foreground_nix_build(line: &str) -> Option<&'static str> {
    if line.len() > MAX_COMMAND
        || line.chars().any(|ch| {
            matches!(
                ch,
                '\n' | '\r' | ';' | '|' | '&' | '(' | ')' | '`' | '$' | '\\' | '!' | '{' | '}'
            )
        })
    {
        return None;
    }
    let tokens = tokenize(line)?;
    let mut index = 0;
    while index < tokens.len() && is_assignment(&tokens[index]) {
        index += 1;
    }
    let mut saw_command = false;
    let mut saw_time = false;
    while index < tokens.len() {
        match tokens[index].as_str() {
            "command" if !saw_command => {
                saw_command = true;
                index += 1;
            }
            "time" if !saw_time => {
                saw_time = true;
                index += 1;
            }
            _ => break,
        }
    }
    while index < tokens.len() && is_assignment(&tokens[index]) {
        index += 1;
    }
    let program = program_name(tokens.get(index)?);
    if program == "nix-build" {
        return Some("nix-build");
    }
    if program == "nix" && tokens.get(index + 1).is_some_and(|token| token == "build") {
        return Some("nix build");
    }
    None
}

/// `focused == Some(false)` is the only case that notifies. An unknown focus
/// stays quiet, and so does an SSH session, whose terminal is not this desktop.
pub fn notice(command: &str, status: i32, focused: Option<bool>, remote: bool) -> Option<Notice> {
    if remote || focused != Some(false) {
        return None;
    }
    let label = foreground_nix_build(command)?;
    if status == 0 {
        Some(Notice {
            title: "Build finished",
            body: label.to_owned(),
        })
    } else {
        Some(Notice {
            title: "Build failed",
            body: format!("{label} exited {status}"),
        })
    }
}

/// Whether `active` is `pid` or one of its ancestors. `None` means the chain
/// could not be read, which the notifier treats as unknown rather than unfocused.
pub fn ancestor_focused(proc_root: &Path, pid: u32, active: u32) -> Option<bool> {
    if pid == 0 || active == 0 {
        return None;
    }
    let mut current = pid;
    let mut seen = [0u32; MAX_ANCESTORS];
    for step in 0..MAX_ANCESTORS {
        if seen[..step].contains(&current) {
            return None;
        }
        seen[step] = current;
        if current == active {
            return Some(true);
        }
        let stat = fs::read_to_string(proc_root.join(current.to_string()).join("stat")).ok()?;
        let parent = ppid(&stat)?;
        // Init is not a window. Stopping here is "not focused", not "unknown".
        if parent <= 1 || parent == current {
            return Some(false);
        }
        current = parent;
    }
    Some(false)
}

pub fn proc_root_from(value: Option<&std::ffi::OsStr>) -> PathBuf {
    value
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/proc"))
}

pub fn proc_root() -> PathBuf {
    proc_root_from(env::var_os("SEELE_BUILD_IDLE_PROC").as_deref())
}

pub fn remote_session() -> bool {
    env::var_os("SSH_CONNECTION").is_some() || env::var_os("SSH_TTY").is_some()
}

fn program_name(value: &str) -> &str {
    let name = Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(value);
    name.strip_suffix("-wrapped").unwrap_or(name)
}

fn comm_is_nix(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    matches!(text.trim(), "nix" | "nix-build" | "nix-wrapped")
}

fn cmdline(path: &Path) -> Option<Vec<String>> {
    let file = fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(65_537).read_to_end(&mut bytes).ok()?;
    if bytes.len() > 65_536 {
        return None;
    }
    Some(
        bytes
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect(),
    )
}

fn is_assignment(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    matches!(chars.next(), Some(ch) if ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn tokenize(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoting = None;
    let mut any = false;
    for ch in line.chars() {
        if let Some(quote) = quoting {
            if ch == quote {
                quoting = None;
            } else {
                current.push(ch);
                any = true;
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quoting = Some(ch);
                any = true;
            }
            ch if ch.is_whitespace() => {
                if any {
                    tokens.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            ch => {
                current.push(ch);
                any = true;
            }
        }
    }
    if quoting.is_some() {
        return None;
    }
    if any {
        tokens.push(current);
    }
    Some(tokens)
}

fn ppid(stat: &str) -> Option<u32> {
    let end = stat.rfind(')')?;
    let mut fields = stat[end + 1..].split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

pub enum ActiveWindow {
    Pid(u32),
    /// A desktop with no focused window. The terminal is not it.
    Empty,
    /// hyprctl could not be read, so focus stays unknown.
    Unknown,
}

/// The focused Hyprland window, when `hyprctl activewindow -j` can say.
pub fn active_window(hyprctl: &Path) -> ActiveWindow {
    let Ok(output) = capture(
        Command::new(hyprctl).args(["activewindow", "-j"]),
        &[],
        Limits {
            timeout: Duration::from_secs(2),
            output: 65_536,
        },
        &std::sync::atomic::AtomicBool::new(false),
    ) else {
        return ActiveWindow::Unknown;
    };
    if !output.status.success() {
        return ActiveWindow::Unknown;
    }
    let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) else {
        return ActiveWindow::Unknown;
    };
    if !value.is_object() {
        return ActiveWindow::Unknown;
    }
    match value.get("pid").and_then(Value::as_u64) {
        Some(pid) if (1..=u64::from(u32::MAX)).contains(&pid) => ActiveWindow::Pid(pid as u32),
        _ => ActiveWindow::Empty,
    }
}

pub fn send_notice(notify: &Path, notice: &Notice) -> Result<(), String> {
    let output = capture(
        Command::new(notify).args([
            "--app-name=Seele",
            "--urgency=normal",
            "--expire-time=30000",
            notice.title,
            &notice.body,
        ]),
        &[],
        Limits {
            timeout: Duration::from_secs(2),
            output: 16_384,
        },
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

pub fn main() -> std::process::ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("active") if args.len() == 1 => match nix_build_running(&proc_root()) {
            Some(true) => {
                println!("running");
                std::process::ExitCode::SUCCESS
            }
            Some(false) => {
                println!("idle");
                std::process::ExitCode::from(1)
            }
            None => {
                eprintln!("seele-build-idle: could not read the process list");
                std::process::ExitCode::from(2)
            }
        },
        Some("notify") => notify(&args[1..]),
        _ => {
            eprintln!(
                "Use: seele-build-idle active | notify --status <code> --pid <pid> --hyprctl <path> --notify <path> -- <command>"
            );
            std::process::ExitCode::from(2)
        }
    }
}

fn notify(args: &[String]) -> std::process::ExitCode {
    let Some(request) = parse_notify(args) else {
        eprintln!(
            "Use: seele-build-idle notify --status <code> --pid <pid> --hyprctl <path> --notify <path> -- <command>"
        );
        return std::process::ExitCode::from(2);
    };
    let focused = if remote_session() {
        None
    } else {
        match active_window(&request.hyprctl) {
            ActiveWindow::Pid(active) => ancestor_focused(&proc_root(), request.pid, active),
            ActiveWindow::Empty => Some(false),
            ActiveWindow::Unknown => None,
        }
    };
    let Some(notice) = notice(&request.command, request.status, focused, remote_session()) else {
        return std::process::ExitCode::SUCCESS;
    };
    match send_notice(&request.notify, &notice) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("seele-build-idle: {error}");
            std::process::ExitCode::from(1)
        }
    }
}

struct NotifyRequest {
    status: i32,
    pid: u32,
    hyprctl: PathBuf,
    notify: PathBuf,
    command: String,
}

fn parse_notify(args: &[String]) -> Option<NotifyRequest> {
    let mut status = None;
    let mut pid = None;
    let mut hyprctl = None;
    let mut notify = None;
    let mut command = None;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        if arg == "--" {
            command = Some(args[index + 1..].join(" "));
            break;
        }
        let value = args.get(index + 1)?;
        match arg {
            "--status" => status = Some(value.parse::<i32>().ok()?),
            "--pid" => {
                let parsed = value.parse::<u32>().ok()?;
                if parsed == 0 {
                    return None;
                }
                pid = Some(parsed);
            }
            "--hyprctl" => hyprctl = Some(tool_path(value)?),
            "--notify" => notify = Some(tool_path(value)?),
            _ => return None,
        }
        index += 2;
    }
    Some(NotifyRequest {
        status: status?,
        pid: pid?,
        hyprctl: hyprctl?,
        notify: notify?,
        command: command?,
    })
}

fn tool_path(value: &str) -> Option<PathBuf> {
    if value.is_empty() || value.contains('\0') || value.contains('\n') {
        return None;
    }
    Some(PathBuf::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn only_a_nix_build_argv_counts() {
        assert!(is_nix_build(&args(&[
            "/nix/store/xx-nix/bin/nix",
            "build",
            ".#foo"
        ])));
        assert!(is_nix_build(&args(&["nix-wrapped", "build"])));
        assert!(is_nix_build(&args(&[
            "/run/current-system/sw/bin/nix-build"
        ])));
        assert!(!is_nix_build(&args(&["nix", "eval"])));
        assert!(!is_nix_build(&args(&["nix", "--offline", "build"])));
        assert!(!is_nix_build(&args(&["nix", "shell", "nixpkgs#hello"])));
        assert!(!is_nix_build(&args(&["cargo", "build"])));
        assert!(!is_nix_build(&args(&["bash", "-lc", "nix build"])));
    }

    #[test]
    fn a_readable_nix_build_is_running_and_a_finished_scan_is_idle() {
        let root = tempfile::tempdir().unwrap();
        let proc_root = root.path();
        assert_eq!(nix_build_running(proc_root), Some(false));
        write_process(proc_root, 7, b"nix\0eval\0", "nix");
        assert_eq!(nix_build_running(proc_root), Some(false));
        write_process(proc_root, 42, b"nix\0build\0.#fixture\0", "nix");
        assert_eq!(nix_build_running(proc_root), Some(true));
    }

    #[test]
    fn an_unreadable_nix_command_line_is_not_treated_as_idle() {
        let root = tempfile::tempdir().unwrap();
        let proc_root = root.path();
        let dir = proc_root.join("9");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("comm"), "nix-build\n").unwrap();
        assert_eq!(nix_build_running(proc_root), Some(true));
    }

    #[test]
    fn a_process_list_that_cannot_be_read_is_not_idle() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(nix_build_running(&root.path().join("missing")), None);
    }

    #[test]
    fn the_proc_override_has_to_be_absolute() {
        assert_eq!(
            proc_root_from(Some(std::ffi::OsStr::new("relative"))),
            PathBuf::from("/proc")
        );
        assert_eq!(
            proc_root_from(Some(std::ffi::OsStr::new("/tmp/seele-proc"))),
            PathBuf::from("/tmp/seele-proc")
        );
        assert_eq!(proc_root_from(None), PathBuf::from("/proc"));
    }

    #[test]
    fn foreground_commands_are_the_job_and_compound_lines_are_not() {
        assert_eq!(foreground_nix_build("nix build .#foo"), Some("nix build"));
        assert_eq!(
            foreground_nix_build("  FOO=1 command time nix build --no-link"),
            Some("nix build")
        );
        assert_eq!(
            foreground_nix_build("/run/current-system/sw/bin/nix-build ./release.nix"),
            Some("nix-build")
        );
        assert_eq!(
            foreground_nix_build("nix build \"./#foo bar\""),
            Some("nix build")
        );
        for line in [
            "echo nix build",
            "nix eval",
            "nix --offline build",
            "nix build && echo done",
            "nix build &",
            "nix build | cat",
            "sudo nix build",
            "nix build $(echo hi)",
            "nix build \"unterminated",
        ] {
            assert_eq!(foreground_nix_build(line), None, "{line}");
        }
    }

    #[test]
    fn a_notification_fires_once_for_an_unfocused_terminal() {
        let finished = notice("nix build .#foo", 0, Some(false), false).unwrap();
        assert_eq!(finished.title, "Build finished");
        assert_eq!(finished.body, "nix build");
        let failed = notice("nix-build ./release.nix", 1, Some(false), false).unwrap();
        assert_eq!(failed.title, "Build failed");
        assert_eq!(failed.body, "nix-build exited 1");
        assert!(notice("nix build", 0, Some(true), false).is_none());
        assert!(notice("nix build", 1, None, false).is_none());
        assert!(notice("nix build", 1, Some(false), true).is_none());
        assert!(notice("echo nix build", 1, Some(false), false).is_none());
    }

    #[test]
    fn focus_follows_the_terminal_ancestor_and_stops_when_the_chain_is_unreadable() {
        let root = tempfile::tempdir().unwrap();
        let proc_root = root.path();
        write_stat(proc_root, 30, "30 (fish) S 20 30 0");
        write_stat(proc_root, 20, "20 (tmux: server) S 10 20 0");
        write_stat(proc_root, 10, "10 (ghostty) S 1 10 0");
        assert_eq!(ancestor_focused(proc_root, 30, 10), Some(true));
        assert_eq!(ancestor_focused(proc_root, 30, 99), Some(false));
        assert_eq!(ancestor_focused(proc_root, 31, 10), None);
    }

    fn write_process(proc_root: &Path, pid: u32, cmdline: &[u8], comm: &str) {
        let dir = proc_root.join(pid.to_string());
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("cmdline"), cmdline).unwrap();
        fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
    }

    fn write_stat(proc_root: &Path, pid: u32, stat: &str) {
        let dir = proc_root.join(pid.to_string());
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("stat"), stat).unwrap();
    }
}
