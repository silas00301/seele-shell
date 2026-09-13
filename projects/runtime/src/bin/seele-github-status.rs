use std::sync::atomic::Ordering;
fn main() -> seele_runtime::Result {
    let signal = seele_runtime::process::termination_signal()?;
    let result = seele_runtime::github::run(&signal);
    let received = signal.load(Ordering::Relaxed);
    if received != 0 {
        std::process::exit(128 + received as i32);
    }
    println!("{result}");
    Ok(())
}
