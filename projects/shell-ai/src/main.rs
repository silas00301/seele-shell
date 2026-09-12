use seele_shell_ai::{capture, context, suggestions, Result, MAX_REQUEST_CHARS};
use serde_json::json;
use std::{env, path::Path};
fn main() {
    match run() {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("seele-shell-ai: {error}");
            std::process::exit(1);
        }
    }
}
fn run() -> Result<i32> {
    let arguments: Vec<_> = env::args().skip(1).collect();
    let mode = arguments
        .first()
        .map(String::as_str)
        .ok_or("expected capture, should-capture, begin, finish, context or suggest")?;
    match mode {
        "capture" if arguments.len() == 3 && arguments[1] == "--fish" => {
            capture::capture_fish(Path::new(&arguments[2]))
        }
        "should-capture" if arguments.len() == 3 && arguments[1] == "--pid" => Ok(
            if arguments[2]
                .parse::<u32>()
                .ok()
                .is_some_and(capture::should_capture)
            {
                0
            } else {
                1
            },
        ),
        "begin" if arguments.len() == 1 => {
            capture::request(&json!({"op":"begin"}), false)?;
            Ok(0)
        }
        "finish" => {
            if arguments.len() != 5 {
                return Err("finish requires command and status");
            }
            let mut command = None;
            let mut status = None;
            for pair in arguments[1..].as_chunks::<2>().0 {
                match pair[0].as_str() {
                    "--command" if command.is_none() => command = Some(pair[1].as_str()),
                    "--status" if status.is_none() => status = pair[1].parse::<i32>().ok(),
                    _ => return Err("invalid finish arguments"),
                }
            }
            capture::request(
                &json!({"op":"finish","command":seele_shell_ai::clean_display(command.ok_or("missing command")?,4096),"status":status.ok_or("missing status")?}),
                false,
            )?;
            Ok(0)
        }
        "context" if arguments.len() == 1 => {
            println!("{}", context::current());
            Ok(0)
        }
        "suggest" if arguments.len() >= 3 && arguments[1] == "--mode" => {
            let mode = &arguments[2];
            let mut tail = &arguments[3..];
            if tail.first().is_some_and(|s| s == "--") {
                tail = &tail[1..];
            }
            let request = tail.join(" ");
            if request.chars().count() > MAX_REQUEST_CHARS {
                return Err("request is too long");
            }
            if mode == "how" && request.trim().is_empty() {
                return Err("usage: how <describe the command you need>");
            }
            if !["how", "debug"].contains(&mode.as_str()) {
                return Err("unknown assistance mode");
            }
            let cancel = seele_runtime::process::termination_signal()
                .map_err(|_| "could not monitor cancellation")?;
            let failure = if mode == "debug" {
                Some(capture::load_failure()?)
            } else {
                None
            };
            let request = suggestions::build_request(mode, &request, context::current(), failure)?;
            let values = suggestions::invoke(
                &seele_runtime::inference::default_socket(),
                &request,
                &cancel,
            )?;
            let selected = suggestions::select(&values, &cancel)?;
            eprintln!("→ {}", selected.description);
            if selected.destructive {
                eprintln!("⚠ destructive command inserted as a comment; review and uncomment it deliberately");
            }
            println!("{}", suggestions::format_insertion(&selected));
            Ok(0)
        }
        _ => Err("invalid shell-assistance arguments"),
    }
}
