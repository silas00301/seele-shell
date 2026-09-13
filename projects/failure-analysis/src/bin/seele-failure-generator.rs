fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.len() != 3 {
        eprintln!("usage: seele-failure-generator NORMAL EARLY LATE");
        std::process::exit(2);
    }
    if seele_failure_analysis::generator::generate(std::path::Path::new(&args[0])).is_err() {
        eprintln!("seele-failure-generator: operation failed");
        std::process::exit(1);
    }
}
