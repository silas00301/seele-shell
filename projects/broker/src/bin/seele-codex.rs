use seele_broker::{
    lifecycle::{Broker, Config},
    runner::Codex,
    transport, validation, MAX_MESSAGE,
};
use serde_json::{json, Value};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn main() {
    // Payloads must not escape through panic messages, backtraces or core dumps.
    std::panic::set_hook(Box::new(|_| eprintln!("broker runtime failure")));
    if run().is_err() {
        eprintln!("broker requires valid configuration and its managed user socket");
        std::process::exit(1);
    }
}
fn run() -> io::Result<()> {
    let stop = seele_runtime::process::termination_signal()?;
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or(io::ErrorKind::InvalidInput)?;
    if !matches!(mode.as_str(), "serve" | "request" | "call") {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let runtime =
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/nonexistent".into()));
    let mut path = runtime.join("seele-codex.sock");
    let mut config = Config::default();
    let mut idle = 300;
    while let Some(arg) = args.next() {
        let value = args.next().ok_or(io::ErrorKind::InvalidInput)?;
        match arg.as_str() {
            "--socket" => path = PathBuf::from(value),
            "--model" if validation::identifier(&value) => config.model = value,
            "--concurrency" => {
                config.concurrency = value
                    .parse::<usize>()
                    .map_err(|_| io::ErrorKind::InvalidInput)?
                    .clamp(1, 8)
            }
            "--idle" => {
                idle = value
                    .parse::<u64>()
                    .ok()
                    .filter(|v| *v > 0)
                    .ok_or(io::ErrorKind::InvalidInput)?
            }
            _ => return Err(io::ErrorKind::InvalidInput.into()),
        }
    }
    if mode != "serve" {
        let mut bytes = Vec::new();
        io::stdin()
            .take((MAX_MESSAGE + 1) as u64)
            .read_to_end(&mut bytes)?;
        let message = if bytes.len() <= MAX_MESSAGE {
            serde_json::from_slice::<Value>(&bytes).ok()
        } else {
            None
        };
        let result = if let Some(message) = message {
            if mode == "call" {
                seele_runtime::inference::call(&path, &message, Duration::from_secs(3600), &stop)
            } else {
                transport::rpc(&path, &message, &stop)
            }
        } else {
            json!({"ok":false,"error":"invalid_input"})
        };
        println!("{result}");
        return Ok(());
    }
    let codex = Codex::new(
        std::env::var_os("SEELE_BROKER_CODEX")
            .unwrap_or_else(|| "codex".into())
            .into(),
        runtime,
    );
    let broker = Broker::new(config, Arc::new(codex));
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(16)
        .enable_all()
        .build()?
        .block_on(async {
            let listener = transport::inherited_listener(&path)?;
            transport::serve(listener, broker, stop, Duration::from_secs(idle)).await
        })
}
