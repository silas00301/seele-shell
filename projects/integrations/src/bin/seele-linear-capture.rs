#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let args = std::env::args().skip(1).collect();
    let result = tokio::select! {result=seele_integrations::linear::run(args,cancel.clone())=>result,_=async{let mut term=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("termination signal");tokio::select!{_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}}=>{cancel.cancel();Err("Linear capture canceled; local files are unchanged.")}};
    if let Err(message) = result {
        eprintln!("{message}");
        std::process::exit(1)
    }
}
