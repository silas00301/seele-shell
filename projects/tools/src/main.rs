mod agents;
mod bluetooth;
mod clock;
mod command;
mod control;
mod grain;
mod launch;
mod mic_sync;
mod receiver;
mod session;
mod shellctl;

use std::env;
use std::error::Error;
use std::path::Path;

pub type Result<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result {
    let mut args: Vec<String> = env::args().collect();
    let executable = Path::new(&args[0])
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("seele-tools");
    let command = match executable {
        "seele-agent-state" => Some("agent-state"),
        "seele-agent" => Some("agent-launch"),
        "seele-agent-run" => Some("agent-run"),
        "seele-agent-hook" => Some("agent-hook"),
        "seele-control" => Some("control"),
        "seele-bt-receiver" => Some("bt-receiver"),
        "seele-bt-agent" => Some("bt-agent"),
        "seele-mic-sync" => Some("mic-sync"),
        "seele-os-session" => Some("os-session"),
        "seele-shellctl" => Some("shellctl"),
        "seele-clock" => Some("clock"),
        "seele-yubikey-watch" => Some("yubikey-watch"),
        "seele-lock-run" => Some("lock-run"),
        "seele-greeter-run" => Some("greeter-run"),
        "seele-grain" => Some("grain"),
        _ => None,
    };
    let command = if let Some(command) = command {
        command.to_owned()
    } else if args.len() > 1 {
        args.remove(1)
    } else {
        return Err("usage: seele-tools <command> [arguments]".into());
    };
    let arguments = args.into_iter().skip(1).collect::<Vec<_>>();

    match command.as_str() {
        "agent-state" => agents::state(&arguments),
        "agent-launch" => agents::launch(&arguments),
        "agent-run" => agents::run_agent(&arguments),
        "agent-hook" => agents::hook(&arguments),
        "control" => control::run(&arguments),
        "bt-receiver" => receiver::run(),
        "bt-agent" => bluetooth::agent(&arguments),
        "mic-sync" => mic_sync::run(&arguments),
        "os-session" => session::run(&arguments),
        "shellctl" => shellctl::run(&arguments),
        "clock" => clock::run(&arguments),
        "yubikey-watch" => bluetooth::watch_yubikey(),
        "lock-run" => launch::lock(&arguments),
        "greeter-run" => launch::greeter(&arguments),
        "grain" => grain::run(&arguments),
        _ => Err(format!("unknown Seele tool: {command}").into()),
    }
}
