#[path = "../color/mod.rs"]
mod color;

fn main() {
    if let Err(error) = color::run() {
        eprintln!("seele-color-worker: {error}");
        std::process::exit(1);
    }
}
