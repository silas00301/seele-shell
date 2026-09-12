use seele_integrations::{common, home_assistant};
use serde_json::json;
use std::process::ExitCode;
use tokio_util::sync::CancellationToken;
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("termination signal");
        tokio::select! { _=term.recv()=>{}, _=tokio::signal::ctrl_c()=>{} }
        signal.cancel();
    });
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["watch"] {
        return if home_assistant::watch(cancel).await.is_err() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        };
    }
    let result = tokio::select! {
        result=home_assistant::run(&args,cancel.clone())=>result,
        _=tokio::time::sleep(std::time::Duration::from_secs(12))=>{cancel.cancel();Err("Home Assistant took too long to respond.")},
        _=cancel.cancelled()=>Err("Home Assistant is unavailable.")
    };
    let (value, code) = match result {
        Ok(value) => (value, ExitCode::SUCCESS),
        Err(error) => (
            json!({"configured":true,"connected":false,"error":error}),
            ExitCode::FAILURE,
        ),
    };
    let _ = common::emit(&value).await;
    // Returning lets Tokio join cancellation-aware blocking subprocess owners;
    // process::exit would bypass their kill-and-reap cleanup.
    code
}
