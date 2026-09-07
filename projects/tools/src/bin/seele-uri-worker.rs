#[path = "../uri/mod.rs"]
mod uri;

fn main() {
    if let Err(error) = uri::run() {
        eprintln!("seele-uri-worker: {error}");
        std::process::exit(1);
    }
}
