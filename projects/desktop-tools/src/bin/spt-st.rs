fn main() {
    let value = seele_runtime::process::termination_signal()
        .map(|cancel| seele_desktop_tools::spotify::status(&cancel))
        .unwrap_or_default();
    println!("{value}");
}
