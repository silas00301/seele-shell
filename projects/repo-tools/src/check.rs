use crate::{capture, interactive, Result};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
pub fn run(args: &[String], cancel: &AtomicUsize) -> Result<()> {
    let mut build = false;
    for arg in args {
        match arg.as_str() {
            "--build" => build = true,
            "-h" | "--help" => {
                println!("Usage: nix run .#check -- [--build]\n\nRun from the Seele checkout root. Formats the whole repository, checks\nflake outputs, and evaluates the native host. --build also builds it.\nKeeps flake.lock unchanged and does not activate the system.");
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {arg}\nTry --help.");
                return Err(2);
            }
        }
    }
    if !Path::new("flake.nix").is_file() || !Path::new("modules/hosts").is_dir() {
        eprintln!("Run this command from the Seele checkout root.");
        return Err(2);
    }
    let system = capture(
        Command::new("nix").args([
            "eval",
            "--impure",
            "--raw",
            "--expr",
            "builtins.currentSystem",
        ]),
        cancel,
    )?;
    let host = match system.trim() {
        "x86_64-linux" => Some(".#nixosConfigurations.nerv.config.system.build.toplevel"),
        "aarch64-darwin" => Some(".#darwinConfigurations.asuka.system"),
        _ => None,
    };
    if build && host.is_none() {
        eprintln!("No native Seele host is defined for {}.", system.trim());
        return Err(2);
    }
    println!("Formatting the repository…");
    interactive(
        Command::new("nix").args(["fmt", "--no-write-lock-file"]),
        cancel,
    )?;
    println!("Checking flake outputs…");
    interactive(
        Command::new("nix").args(["flake", "show", "--no-write-lock-file"]),
        cancel,
    )?;
    interactive(
        Command::new("nix").args(["flake", "check", "--no-build", "--no-write-lock-file"]),
        cancel,
    )?;
    if let Some(host) = host {
        println!("Evaluating the {} host…", system.trim());
        interactive(
            Command::new("nix").args([
                "eval",
                "--raw",
                &format!("{host}.drvPath"),
                "--no-write-lock-file",
            ]),
            cancel,
        )?;
        println!();
        if build {
            println!("Building the native host…");
            interactive(
                Command::new("nix").args(["build", host, "--no-link", "--no-write-lock-file"]),
                cancel,
            )?;
        }
    } else {
        println!(
            "No native host for {}; portable outputs were checked.",
            system.trim()
        );
    }
    println!("Seele validation completed.");
    Ok(())
}
