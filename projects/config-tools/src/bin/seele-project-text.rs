fn main() {
    let result = seele_runtime::process::termination_signal()
        .map_err(Into::into)
        .and_then(|cancel| {
            seele_config_tools::project_text::run(
                &std::env::args().skip(1).collect::<Vec<_>>(),
                &cancel,
            )
        });
    match result {
        Ok(status) => std::process::exit(status),
        Err(_) => {
            eprintln!("seele-project-text: search or file selection failed");
            std::process::exit(2);
        }
    }
}
