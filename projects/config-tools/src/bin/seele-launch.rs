fn main() -> seele_config_tools::Result {
    let mut arguments = std::env::args_os().skip(1);
    let manifest = arguments
        .next()
        .ok_or("usage: seele-launch MANIFEST [ARGUMENT ...]")?;
    seele_config_tools::launch::run(manifest.as_ref(), arguments)
}
