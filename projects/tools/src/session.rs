use crate::command::{exec, home, output, status};
use crate::Result;
use std::env;
use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

fn variable(name: &str, fallback: &str) -> String {
    env::var(name).unwrap_or_else(|_| fallback.into())
}
fn tty_line(prompt: &str) -> Option<String> {
    print!("{prompt}");
    io::stdout().flush().ok()?;
    let file = File::open("/dev/tty").ok()?;
    let mut line = String::new();
    io::BufReader::new(file).read_line(&mut line).ok()?;
    Some(line.trim().to_owned())
}
fn confirm(prompt: &str) -> bool {
    tty_line(&format!("{prompt} [y/N] "))
        .map(|value| value.to_ascii_lowercase().starts_with('y'))
        .unwrap_or(false)
}
fn repository_changes(repo: &Path) -> String {
    output("jj", ["diff", "--summary"])
        .or_else(|| {
            output(
                "git",
                ["-C", &repo.to_string_lossy(), "status", "--porcelain"],
            )
        })
        .unwrap_or_default()
}
fn commit_message(repo: &Path, pi: &str) -> String {
    let recent = output(
        "jj",
        [
            "log",
            "--no-graph",
            "-r",
            "latest(::@- & ~empty(), 8)",
            "-T",
            "description.first_line() ++ \"\\n\"",
        ],
    )
    .or_else(|| {
        output(
            "git",
            ["-C", &repo.to_string_lossy(), "log", "-8", "--format=%s"],
        )
    })
    .unwrap_or_default();
    let diff = output("jj", ["diff", "--stat"])
        .or_else(|| output("git", ["-C", &repo.to_string_lossy(), "diff", "--stat"]))
        .unwrap_or_default();
    let prompt = format!("Write the commit message for this change to the Seele Nix flake.\n\nMatch the style of the repository's recent subjects:\n{recent}\nRules: one line, imperative mood, no trailing period, no conventional-commit prefix, no quotes around it, at most 72 characters. Reply with the subject line and nothing else.\n\nChanged files:\n{diff}");
    output(pi, ["-p", "--no-session", "--no-tools", &prompt])
        .unwrap_or_default()
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_owned()
}
fn session(repo: &Path) -> Result {
    env::set_current_dir(repo)?;
    print!("\x1b]0;Seele OS session\x07");
    println!("Seele OS session · {}\nDescribe the change you want. The system rebuilds when you exit Pi.\n", repo.display());
    let pi = variable("SEELE_SHELL_PI", "pi");
    let briefing = format!("You are running inside the Seele desktop shell's OS session, opened from the AI cockpit in {}.\n\nThe user describes a change they want to their own desktop or system. Implement it in this repository, following AGENTS.md and the seele skill under .agents/skills/.\n\nDo not activate the system and do not commit: when you finish implementing, say so and end your turn. This session then runs 'nh os switch' and offers to record the change with Jujutsu.", repo.display());
    let _ = Command::new("seele-agent-run")
        .args([
            "pi",
            &pi,
            "--name",
            "Seele OS session",
            "--append-system-prompt",
            &briefing,
        ])
        .status();
    if repository_changes(repo).trim().is_empty() {
        println!("\nNo changes in the working copy, so nothing to build.");
        let _ = tty_line("Press enter to close. ");
        return Ok(());
    }
    println!("\nRebuilding the system with nh os switch.");
    if !status(&variable("SEELE_SHELL_NH", "nh"), ["os", "switch"]) {
        println!("\nThe rebuild failed. The working copy is untouched, so you can reopen this session and keep going.");
        let _ = tty_line("Press enter to close. ");
        return Err("system rebuild failed".into());
    }
    println!("\nThe system is running the new configuration.");
    if confirm("Record this change?") {
        println!("Writing a commit message...");
        let message = commit_message(repo, &pi);
        if message.is_empty() {
            println!("Could not generate a message. Nothing was committed.");
        } else {
            println!("\n  {message}\n");
            if confirm("Commit with this message?") {
                status("jj", ["commit", "-m", &message]);
                println!("Committed.");
            } else {
                println!("Nothing was committed.");
            }
        }
    }
    let _ = tty_line("Press enter to close. ");
    Ok(())
}
pub fn run(arguments: &[String]) -> Result {
    let repo = env::var_os("SEELE_SHELL_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("seele"));
    match arguments.first().map(String::as_str).unwrap_or("open") {
        "open" => {
            status(
                &variable("SEELE_SHELL_HYPRCTL", "hyprctl"),
                [
                    "dispatch",
                    "workspace",
                    &variable("SEELE_SHELL_OS_WORKSPACE", "9"),
                ],
            );
            exec(
                &variable("SEELE_SHELL_GHOSTTY", "ghostty"),
                &[
                    format!("--working-directory={}", repo.display()),
                    "--class=org.seele.os-session".into(),
                    "-e".into(),
                    "seele-os-session".into(),
                    "session".into(),
                ],
            )
        }
        "session" => session(&repo),
        _ => Err("Usage: seele-os-session [open|session]".into()),
    }
}
