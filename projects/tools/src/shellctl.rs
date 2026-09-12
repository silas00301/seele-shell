use crate::command::{detached, exec, output};
use crate::Result;
use std::env;
use std::io::Read;
use std::process::Command;

const USAGE: &str = r#"Usage: seele-shellctl [-q] <command> [arguments]

Commands:
  menu [apps|commands]      Toggle the launcher
  agents                    Toggle the AI dashboard
  prompt                    Toggle the quick AI prompt
  center                    Toggle the Control Center
  transfers                 Open personal Transfers
  controls                  Toggle session controls
  uris                      Freeze all screens and pick a visible URI
  control <panel>           Toggle a control panel
  bluetooth-pairing <json>  Show a Bluetooth pairing request
  bluetooth-pairing-dismiss Withdraw the Bluetooth pairing request
  agent <name> [prompt...]  Launch an agent
  refresh-agents            Refresh AI usage data
  volume <up|down|mute>     Change volume and show its OSD
  microphone <up|down|mute> Change the microphone and show its OSD
  microphone-state <muted|live> Show a device mute OSD
  notes                     Open Seele Notes for quick capture into the vault
  voxtype                   Toggle voice dictation
  lock                      Lock the session
  notification <action> [id] [key]  Invoke, dismiss, retire, pin, clear, clear-history, dnd, or snooze <minutes>
  health-publish <id>       Publish bounded health JSON from stdin
  health-status             Print registered current health metadata
  notification-status       Print notification state as JSON
  ping                      Check shell IPC
"#;

fn ipc_output(arguments: &[String]) -> Result<String> {
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
    output("quickshell", &args).ok_or_else(|| "Seele Shell is not responding".into())
}

fn ipc(quiet: bool, arguments: &[String]) -> Result {
    match ipc_output(arguments) {
        Ok(value) => {
            if !quiet && !value.trim().is_empty() { println!("{}", value.trim_end()); }
            Ok(())
        }
        Err(_) if quiet => Ok(()),
        Err(error) => Err(error),
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
        "prompt" => call("togglePrompt", &[]),
        "center" => call("toggleControl", &["control-center".into()]),
        "transfers" => call("openTransfers", &[]),
        "controls" => call("toggleControls", &[]),
        "uris" => call("toggleUris", &[]),
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
        "microphone" if rest.first().map(String::as_str) == Some("mute") => {
            // Do not let a full system-status collection delay the mute OSD
            // or deliver an obsolete snapshot after a subsequent key press.
            let status = Command::new("seele-control")
                .args(["microphone", "mute"])
                .env("SEELE_CONTROL_NO_STATUS", "1")
                .status()?;
            if !status.success() {
                return Err("audio control failed".into());
            }
            let state = output("wpctl", ["get-volume", "@DEFAULT_AUDIO_SOURCE@"])
                .ok_or("microphone state unavailable")?;
            let muted = if state.contains("MUTED") {
                "muted"
            } else {
                "live"
            };
            call("showMicrophone", &[muted.into()])?;
            call("refreshStatus", &[])
        }
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
        "notes" => detached("seele-notes", &[]),
        "voxtype" => {
            let result = output("seele-control", ["voxtype"]).ok_or("voxtype control failed")?;
            call("updateStatus", &[result])
        }
        "lock" => exec("seele-control", &["lock".into()]),
        "health-status" => call("healthStatus", &[]),
        "health-publish" => {
            if rest.len() != 1 { return Err("health provider id required".into()); }
            let mut payload = String::new();
            std::io::stdin().take(4097).read_to_string(&mut payload)?;
            if payload.len() > 4096 { return Err("health payload too large".into()); }
            let response = ipc_output(&["healthPublish".into(), rest[0].clone(), payload])?;
            if response.trim() != "ok" { return Err("health publication rejected".into()); }
            Ok(())
        }
        "notification-status" => call("notificationStatus", &[]),
        "notification" => {
            let action = rest.first().ok_or("notification action required")?;
            let id = rest.get(1).cloned().unwrap_or_default();
            let key = rest.get(2).cloned().unwrap_or_else(|| "default".into());
            let response = ipc_output(&["notificationCommand".into(), action.clone(), id, key])?;
            if response.trim() != "ok" { return Err("notification action unavailable".into()); }
            Ok(())
        }
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
