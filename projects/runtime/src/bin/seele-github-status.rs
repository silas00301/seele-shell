use std::sync::atomic::Ordering;
fn main() -> seele_runtime::Result {
    let signal = seele_runtime::process::termination_signal()?;
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.first().map(String::as_str) {
        None => seele_runtime::github::run(&signal),
        Some("focus") if arguments.len() == 2 => {
            seele_runtime::github::focus(&signal, &arguments[1])
        }
        Some("focus") => {
            serde_json::json!({"state": "error", "message": "Focus reads one pull request."})
        }
        _ => serde_json::json!({"state": "error", "message": "Unknown GitHub status command."}),
    };
    let received = signal.load(Ordering::Relaxed);
    if received != 0 {
        std::process::exit(128 + received as i32);
    }
    println!("{result}");
    Ok(())
}
