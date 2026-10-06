#[path = "../shelf.rs"]
mod shelf;
fn main() {
    std::panic::set_hook(Box::new(|_| eprintln!("shelf unavailable")));
    if shelf::run().is_err() {
        eprintln!("shelf unavailable");
        std::process::exit(1);
    }
}
