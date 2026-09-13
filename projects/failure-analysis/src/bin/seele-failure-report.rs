fn main() {
    std::panic::set_hook(Box::new(|_| {
        eprintln!("seele-failure-report: runtime failure")
    }));
    let result = seele_runtime::process::termination_signal().and_then(|cancel| {
        seele_failure_analysis::ui::main(&std::env::args().skip(1).collect::<Vec<_>>(), &cancel)
    });
    match result {
        Ok(code) => std::process::exit(code),
        Err(_) => {
            eprintln!("seele-failure-report: operation failed");
            std::process::exit(1);
        }
    }
}
