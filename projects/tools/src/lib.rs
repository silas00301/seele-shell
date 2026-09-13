//! Shared implementations behind independently linked native tools.
mod agents;
mod audio;
mod audio_route;
mod bluetooth;
mod clock;
mod command;
mod control;
mod daemon;
mod grain;
mod launch;
mod live;
mod mic_sync;
mod notes;
mod nothing;
mod pipewire;
mod receiver;
mod session;
mod shellctl;
mod vicinae;

use std::{error::Error, path::Path, process::ExitCode};
pub type Result<T = ()> = std::result::Result<T, Box<dyn Error + Send + Sync>>;
#[derive(Clone, Copy)]
pub enum Tool {
    AgentState,
    AgentLaunch,
    AgentRun,
    AgentHook,
    Control,
    BluetoothReceiver,
    BluetoothAgent,
    MicrophoneSync,
    NothingHeadphones,
    OsSession,
    ShellControl,
    Clock,
    NotesStore,
    NotesRun,
    YubikeyWatch,
    Lock,
    Greeter,
    Grain,
}

const TOOLS: &[(&str, &str, Tool)] = &[
    ("agent-state", "seele-agent-state", Tool::AgentState),
    ("agent-launch", "seele-agent", Tool::AgentLaunch),
    ("agent-run", "seele-agent-run", Tool::AgentRun),
    ("agent-hook", "seele-agent-hook", Tool::AgentHook),
    ("control", "seele-control", Tool::Control),
    ("bt-receiver", "seele-bt-receiver", Tool::BluetoothReceiver),
    ("bt-agent", "seele-bt-agent", Tool::BluetoothAgent),
    ("mic-sync", "seele-mic-sync", Tool::MicrophoneSync),
    (
        "nothing-headphones",
        "seele-nothing-headphones",
        Tool::NothingHeadphones,
    ),
    ("os-session", "seele-os-session", Tool::OsSession),
    ("shellctl", "seele-shellctl", Tool::ShellControl),
    ("clock", "seele-clock", Tool::Clock),
    ("notes", "seele-notes-store", Tool::NotesStore),
    ("notes-run", "seele-notes-run", Tool::NotesRun),
    ("yubikey-watch", "seele-yubikey-watch", Tool::YubikeyWatch),
    ("lock-run", "seele-lock-run", Tool::Lock),
    ("greeter-run", "seele-greeter-run", Tool::Greeter),
    ("grain", "seele-grain", Tool::Grain),
];

impl Tool {
    fn run(self, arguments: Vec<String>) -> Result {
        match self {
            Self::AgentState => agents::state(&arguments),
            Self::AgentLaunch => agents::launch(&arguments),
            Self::AgentRun => agents::run_agent(&arguments),
            Self::AgentHook => agents::hook(&arguments),
            Self::Control => control::run(&arguments),
            Self::BluetoothReceiver => receiver::run(),
            Self::BluetoothAgent => bluetooth::agent(&arguments),
            Self::MicrophoneSync => mic_sync::run(&arguments),
            Self::NothingHeadphones => nothing::run(&arguments),
            Self::OsSession => session::run(&arguments),
            Self::ShellControl => shellctl::run(&arguments),
            Self::Clock => clock::run(&arguments),
            Self::NotesStore => notes::run(&arguments),
            Self::NotesRun => launch::notes(&arguments),
            Self::YubikeyWatch => bluetooth::watch_yubikey(),
            Self::Lock => launch::lock(&arguments),
            Self::Greeter => launch::greeter(&arguments),
            Self::Grain => grain::run(&arguments),
        }
    }
}
fn finish(result: Result) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
pub fn entry(tool: Tool) -> ExitCode {
    finish(tool.run(std::env::args().skip(1).collect()))
}
/// Existing `seele-tools command ...` and historical executable aliases retain
/// one compatibility route; production wrappers can use the dedicated binaries.
pub fn compatibility_entry() -> ExitCode {
    let mut args = std::env::args();
    let executable = args.next().unwrap_or_default();
    let executable = Path::new(&executable)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("seele-tools");
    let tool = TOOLS
        .iter()
        .find(|(_, binary, _)| *binary == executable)
        .map(|(_, _, tool)| *tool)
        .or_else(|| {
            let command = args.next()?;
            TOOLS
                .iter()
                .find(|(name, _, _)| *name == command)
                .map(|(_, _, tool)| *tool)
        });
    finish(match tool {
        Some(tool) => tool.run(args.collect()),
        None => Err("usage: seele-tools <command> [arguments]".into()),
    })
}
