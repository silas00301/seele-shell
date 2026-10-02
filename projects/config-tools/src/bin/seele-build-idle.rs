fn main() {
    let code = seele_config_tools::build_idle::main();
    std::process::exit(match code {
        code if code == std::process::ExitCode::SUCCESS => 0,
        code if code == std::process::ExitCode::from(1) => 1,
        _ => 2,
    });
}
