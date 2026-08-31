use crate::command::{exec, output};
use crate::Result;
use std::env;

const USAGE: &str = r#"Usage: seele-shellctl [-q] <command> [arguments]

Commands:
  menu [apps|commands]      Toggle the launcher
  agents                    Toggle the AI dashboard
  center                    Toggle the Control Center
  controls                  Toggle session controls
  control <panel>           Toggle a control panel
  bluetooth-pairing <json>  Show a Bluetooth pairing request
  bluetooth-pairing-dismiss Withdraw the Bluetooth pairing request
  agent <name> [prompt...]  Launch an agent
  refresh-agents            Refresh AI usage data
  volume <up|down|mute>     Change volume and show its OSD
  microphone <up|down|mute> Change the microphone and show its OSD
  microphone-state <muted|live> Show a device mute OSD
  voxtype                   Toggle voice dictation
  lock                      Lock the session
  ping                      Check shell IPC
"#;

fn ipc(quiet: bool, arguments: &[String]) -> Result {
    let shell = env::var("SEELE_SHELL_PATH").map_err(|_| "SEELE_SHELL_PATH is not set")?;
    let mut args = vec![
        "ipc".into(),
        "-n".into(),
        "-p".into(),
        shell,
        "call".into(),
        "--".into(),
        "seele-shell".into(),
    ];
    args.extend_from_slice(arguments);
    match output("quickshell", &args) {
        Some(value) => {
            if !quiet && !value.trim().is_empty() {
                print!("{value}");
                if !value.ends_with('\n') {
                    println!();
                }
            }
            Ok(())
        }
        None if quiet => Ok(()),
        None => Err("Seele Shell is not responding".into()),
    }
}

pub fn run(arguments: &[String]) -> Result {
    let (quiet, arguments) = if arguments.first().map(String::as_str) == Some("-q") {
        (true, &arguments[1..])
    } else {
        (false, arguments)
    };
    let command = arguments.first().map(String::as_str).unwrap_or("--help");
    let rest = arguments.get(1..).unwrap_or_default();
    let call = |method: &str, extra: &[String]| {
        let mut args = vec![method.to_owned()];
        args.extend_from_slice(extra);
        ipc(quiet, &args)
    };
    match command {
        "menu" => call(
            "toggleLauncher",
            &[rest.first().cloned().unwrap_or_else(|| "apps".into())],
        ),
        "agents" => call("toggleAgents", &[]),
        "center" => call("toggleControl", &["control-center".into()]),
        "controls" => call("toggleControls", &[]),
        "control" => call(
            "toggleControl",
            &[rest.first().cloned().unwrap_or_else(|| "system".into())],
        ),
        "bluetooth-pairing" => call(
            "bluetoothPairingRequest",
            &[rest.first().ok_or("request payload required")?.clone()],
        ),
        "bluetooth-pairing-dismiss" => call("bluetoothPairingDismiss", &[]),
        "agent" => {
            let agent = rest.first().cloned().unwrap_or_else(|| "pi".into());
            call(
                "launchAgent",
                &[agent, rest.get(1..).unwrap_or_default().join(" ")],
            )
        }
        "refresh-agents" => call("refreshAgents", &[]),
        "volume" | "microphone" => {
            let action = rest.first().ok_or("audio action required")?.clone();
            let result = output("seele-control", [command, action.as_str()])
                .ok_or("audio control failed")?;
            call("updateStatus", &[result])?;
            if command == "volume" {
                call("showVolume", &[])
            } else {
                call("showMicrophone", &[String::new()])
            }
        }
        "microphone-state" => call(
            "showMicrophone",
            &[rest.first().ok_or("muted or live required")?.clone()],
        ),
        "voxtype" => {
            let result = output("seele-control", ["voxtype"]).ok_or("voxtype control failed")?;
            call("updateStatus", &[result])
        }
        "lock" => exec("seele-control", &["lock".into()]),
        "ping" => call("ping", &[]),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        _ => {
            eprint!("{USAGE}");
            Err("unknown shell command".into())
        }
    }
}
