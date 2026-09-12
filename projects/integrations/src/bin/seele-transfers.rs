use seele_integrations::{common, transfers};
use serde_json::json;
use tokio_util::sync::CancellationToken;
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> std::process::ExitCode {
    unsafe {
        libc::umask(0o077);
    }
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! {_=term.recv()=>{},_=tokio::signal::ctrl_c()=>{}}
        signal.cancel();
    });
    if let Err(error) = transfers::run(&std::env::args().skip(1).collect::<Vec<_>>(), cancel).await
    {
        let _ = common::emit(&json!({"error":error})).await;
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
