//! `rb`: record, check, build, diff and only then, when asked, activate.
//!
//! Every step runs the tool the manual workflow already used, with its output
//! streamed unchanged. The generation that is activated is the store path the
//! diff was taken from, handed to `nh` as a store installable, so nothing is
//! evaluated a second time between reviewing a change and switching to it.
use crate::{executable, run, text::clean, ui};
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::os::fd::FromRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

pub const USAGE: &str = "usage: rb [--dry-run | --switch] [--flake PATH]

Records the Jujutsu working copy, runs the flake checks, builds this host,
shows an nvd diff against the running system and then asks whether to
activate that exact build. --dry-run stops after the diff; --switch activates
without asking. Without a terminal to ask on, nothing is activated.
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activation {
    Ask,
    Never,
    Always,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    pub activation: Activation,
    pub flake: Option<String>,
}
pub fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        activation: Activation::Ask,
        flake: None,
    };
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        let activation = match argument.as_str() {
            "--dry-run" | "-n" => Activation::Never,
            "--switch" | "-s" => Activation::Always,
            "--flake" => {
                let path = arguments.next().ok_or("--flake needs a path")?;
                options.flake = Some(path.clone());
                continue;
            }
            other => return Err(format!("unknown argument: {other}")),
        };
        if options.activation != Activation::Ask && options.activation != activation {
            return Err("--dry-run and --switch exclude each other".into());
        }
        options.activation = activation;
    }
    Ok(options)
}
// The flake `nh` itself is pointed at, so `rb` and a bare `nh os switch` can
// never disagree about which repository describes this machine.
fn flake(options: &Options) -> Option<PathBuf> {
    options
        .flake
        .clone()
        .or_else(|| std::env::var("NH_OS_FLAKE").ok())
        .or_else(|| std::env::var("NH_FLAKE").ok())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

struct Failure {
    step: &'static str,
    command: String,
    code: i32,
    output: String,
}
struct Run<'a> {
    stdout: File,
    cancel: &'a AtomicUsize,
    record: String,
}
impl Run<'_> {
    fn heading(&mut self, text: &str) {
        let _ = writeln!(self.stdout, "\n\x1b[1m==> {text}\x1b[0m");
    }
    fn line(&mut self, text: &str) {
        let _ = writeln!(self.stdout, "{text}");
    }
    // Streams a step byte for byte, keeping only a bounded tail for a report.
    fn stream(&mut self, step: &'static str, command: &mut Command) -> Result<(), Failure> {
        let line = describe(command);
        match seele_runtime::process::tee(
            command,
            crate::MAX_COMMAND_OUTPUT,
            Duration::from_secs(86400),
            self.cancel,
            &mut self.stdout,
        ) {
            Ok((status, _)) if status.success() => Ok(()),
            Ok((status, tail)) => Err(Failure {
                step,
                command: line,
                code: status.code().unwrap_or(1),
                output: String::from_utf8_lossy(&tail).into_owned(),
            }),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => Err(Failure {
                step,
                command: line,
                code: 130,
                output: String::new(),
            }),
            Err(_) => Err(Failure {
                step,
                command: line,
                code: 127,
                output: "The command could not run or exceeded its resource limit".into(),
            }),
        }
    }
}
fn describe(command: &Command) -> String {
    std::iter::once(command.get_program())
        .chain(command.get_args())
        .map(|part| part.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}
fn notify(summary: &str, body: &str, cancel: &AtomicUsize) {
    let mut command = Command::new(executable("SEELE_FAILURE_NOTIFY", "notify-send"));
    command
        .args([
            "--app-name=Seele",
            "--icon=system-software-update",
            "--transient",
            "--",
        ])
        .arg(clean(summary, 100))
        .arg(clean(body, 400));
    // A notification that cannot be shown never changes what the rebuild did.
    let _ = run(&mut command, b"", 10, cancel);
}
fn store_dir() -> PathBuf {
    std::env::var_os("SEELE_REBUILD_STORE_DIR")
        .map_or_else(|| PathBuf::from("/nix/store"), PathBuf::from)
}
fn current_system() -> PathBuf {
    std::env::var_os("SEELE_REBUILD_CURRENT_SYSTEM")
        .map_or_else(|| PathBuf::from("/run/current-system"), PathBuf::from)
}
fn confirm(stdout: &mut File) -> bool {
    // SAFETY: isatty only inspects the descriptor.
    if unsafe { libc::isatty(libc::STDIN_FILENO) } != 1 {
        let _ = writeln!(
            stdout,
            "No terminal to ask on, so nothing was activated. Run rb --switch to activate."
        );
        return false;
    }
    let _ = write!(stdout, "Activate this generation? [y/N] ");
    let _ = stdout.flush();
    let mut answer = String::new();
    if io::stdin().lock().read_line(&mut answer).is_err() {
        return false;
    }
    matches!(answer.trim(), "y" | "Y" | "yes" | "Yes" | "YES")
}

pub fn main(arguments: &[String], cancel: &AtomicUsize) -> io::Result<i32> {
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        print!("{USAGE}");
        return Ok(0);
    }
    let options = match parse(arguments) {
        Ok(options) => options,
        Err(error) => {
            eprint!("rb: {error}\n\n{USAGE}");
            return Ok(2);
        }
    };
    let Some(flake) = flake(&options) else {
        eprintln!("rb: no flake to build. Pass --flake PATH or set NH_FLAKE.");
        return Ok(2);
    };
    // SAFETY: duplicating stdout gives the step writer its own descriptor.
    let fd = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fd is a fresh descriptor owned only by this File.
    let stdout = unsafe { File::from_raw_fd(fd) };
    let mut state = Run {
        stdout,
        cancel,
        record: String::new(),
    };
    match rebuild(&mut state, &options, &flake) {
        Ok(outcome) => {
            notify("NixOS rebuild", &outcome, cancel);
            Ok(0)
        }
        Err(failure) if failure.code == 130 => Ok(130),
        Err(failure) => {
            let last = failure
                .output
                .lines()
                .rev()
                .find(|line| !line.trim().is_empty())
                .map(|line| clean(line, 300))
                .unwrap_or_else(|| format!("Exit status: {}", failure.code));
            state.line(&format!(
                "\nrb stopped at {}: {}\nNothing was activated; the working copy is untouched.",
                failure.step, failure.command
            ));
            let report = format!(
                "Seele failure report\nCollected: {}\nFailed step: {}\nFailed command: {}\nExit status: {}\nWorking copy: {}\n\n[Output from failed step]\n{}\n",
                seele_runtime::time::timestamp(),
                failure.step,
                failure.command,
                failure.code,
                if state.record.is_empty() { "not recorded" } else { &state.record },
                clean(&failure.output, crate::MAX_COMMAND_OUTPUT)
            );
            let _ = ui::store_offer(
                &report,
                "NixOS rebuild",
                &format!("{} failed · exit {}\n{last}", failure.step, failure.code),
                cancel,
            );
            Ok(failure.code)
        }
    }
}

fn rebuild(state: &mut Run<'_>, options: &Options, flake: &Path) -> Result<String, Failure> {
    let flake_arg = flake.as_os_str();
    state.heading("Recording the Jujutsu working copy");
    // Any jj command snapshots the working copy first; this one also names it.
    let mut record = Command::new(executable("SEELE_FAILURE_JJ", "jj"));
    record.arg("--repository").arg(flake_arg).args([
        "--no-pager",
        "log",
        "--no-graph",
        "-r",
        "@",
        "-T",
        r#"change_id.short() ++ " " ++ commit_id.short() ++ " " ++ if(description, description.first_line(), "(no description)")"#,
    ]);
    let line = describe(&record);
    let recorded = run(&mut record, b"", 120, state.cancel);
    if recorded.code != 0 {
        return Err(Failure {
            step: "Recording the working copy",
            command: line,
            code: recorded.code,
            output: recorded.stderr,
        });
    }
    state.record = clean(recorded.stdout.trim(), 300);
    let summary = format!("Working copy {}", state.record);
    state.line(&summary);

    state.heading("Checking the flake");
    let mut check = Command::new(executable("SEELE_FAILURE_NIX", "nix"));
    check
        .args(["flake", "check", "--no-build", "--no-write-lock-file"])
        .arg(flake_arg);
    state.stream("Flake checks", &mut check)?;

    state.heading("Building this host");
    let links = tempfile::Builder::new()
        .prefix("seele-rb-")
        .tempdir_in(std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into()))
        .map_err(|_| Failure {
            step: "Build",
            command: "private result directory".into(),
            code: 1,
            output: "Could not create a private directory for the build result".into(),
        })?;
    let link = links.path().join("result");
    let mut build = Command::new(executable("SEELE_FAILURE_NH", "nh"));
    build
        .args(["os", "build", "--diff", "never", "--out-link"])
        .arg(&link)
        .arg(flake_arg);
    state.stream("Build", &mut build)?;
    let built = std::fs::canonicalize(&link)
        .ok()
        .filter(|path| path.starts_with(store_dir()))
        .ok_or_else(|| Failure {
            step: "Build",
            command: describe(&build),
            code: 1,
            output: "The build left no store path behind its result link".into(),
        })?;

    state.heading("Comparing with the running system");
    let current = std::fs::canonicalize(current_system()).ok();
    if current.as_deref() == Some(built.as_path()) {
        state.line("This build is the running system; there is nothing to activate.");
        return Ok(format!("Nothing to activate · {}", state.record));
    }
    match &current {
        Some(current) => {
            let mut diff = Command::new(executable("SEELE_FAILURE_NVD", "nvd"));
            diff.arg("diff").arg(current).arg(&built);
            // A diff that cannot be drawn is not a reason to withhold the build.
            if state.stream("Diff", &mut diff).is_err() {
                state.line("nvd could not compare the generations.");
            }
        }
        None => state.line("No running system to compare with."),
    }

    let activate = match options.activation {
        Activation::Never => false,
        Activation::Always => true,
        Activation::Ask => confirm(&mut state.stdout),
    };
    if !activate {
        state.line(&format!("Built {} without activating it.", built.display()));
        return Ok(format!("Built, not activated · {}", state.record));
    }

    state.heading("Activating the reviewed build");
    // A store installable skips evaluation, so the generation activated is the
    // one that was diffed even if the working copy changed in the meantime.
    let mut switch = Command::new(executable("SEELE_FAILURE_NH", "nh"));
    switch.args(["os", "switch", "--diff", "never"]).arg(&built);
    state.stream("Activation", &mut switch)?;
    Ok(format!("Activated · {}", state.record))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }
    #[test]
    fn activation_is_asked_unless_chosen() {
        assert_eq!(parse(&[]).unwrap().activation, Activation::Ask);
        assert_eq!(
            parse(&args(&["--dry-run"])).unwrap().activation,
            Activation::Never
        );
        assert_eq!(
            parse(&args(&["-s"])).unwrap().activation,
            Activation::Always
        );
        assert_eq!(
            parse(&args(&["--flake", "/repo", "--switch"])).unwrap(),
            Options {
                activation: Activation::Always,
                flake: Some("/repo".into())
            }
        );
        assert!(parse(&args(&["--dry-run", "--switch"])).is_err());
        assert!(parse(&args(&["--flake"])).is_err());
        assert!(parse(&args(&["boot"])).is_err());
    }
}
