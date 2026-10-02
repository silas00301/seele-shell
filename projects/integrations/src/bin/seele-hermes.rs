use seele_integrations::{common, hermes};
use serde_json::{json, Value};
use tokio::io::BufReader;
use tokio_util::sync::CancellationToken;
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> std::process::ExitCode {
    unsafe {
        libc::umask(0o077);
    }
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    tokio::spawn(async move {
        let mut term =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
        tokio::select! {_=term.recv()=>{},_=tokio::signal::ctrl_c()=>{}}
        stop.cancel();
    });
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = match args.as_slice() {
        [op] if op == "serve" => hermes::serve(cancel.clone()).await,
        [op] if op == "watch" => hermes::request(json!({"op":"watch"}), true, cancel.clone()).await,
        [op] if op == "request" || op == "publish" => {
            let mut input = BufReader::new(common::FdIo::stdin().unwrap());
            match tokio::time::timeout(
                std::time::Duration::from_secs(2),
                common::read_frame(&mut input, 65536),
            )
            .await
            {
                Ok(Ok(Some(bytes))) => match serde_json::from_slice::<Value>(&bytes) {
                    Ok(v) => {
                        hermes::request(
                            if op == "publish" {
                                json!({"op":"publish","report":v})
                            } else {
                                v
                            },
                            false,
                            cancel.clone(),
                        )
                        .await
                    }
                    Err(_) => Err("Invalid Hermes input."),
                },
                _ => Err("Invalid Hermes input."),
            }
        }
        _ => Err("Usage: seele-hermes serve|watch|request|publish"),
    };
    if let Err(error) = result {
        let _ = common::emit(&json!({"ok":false,"error":error})).await;
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
