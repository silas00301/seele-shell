fn main() {
    let result = seele_runtime::process::termination_signal().and_then(|cancel| {
        seele_failure_analysis::ui::rebuild(&std::env::args().skip(1).collect::<Vec<_>>(), &cancel)
    });
    std::process::exit(result.unwrap_or(1));
}
