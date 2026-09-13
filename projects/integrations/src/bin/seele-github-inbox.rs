use seele_integrations::github;
use std::process::ExitCode;
use tokio_util::sync::CancellationToken;
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> ExitCode {
    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    tokio::spawn(async move {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("termination signal");
        tokio::select! {_=term.recv()=>{},_=tokio::signal::ctrl_c()=>{}}
        signal.cancel();
    });
    match github::watch(cancel).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}
