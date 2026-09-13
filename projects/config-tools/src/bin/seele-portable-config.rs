fn main() -> seele_config_tools::Result {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: seele-portable-config SOURCE DESTINATION".into());
    }
    seele_config_tools::materialize::materialize(args[0].as_ref(), args[1].as_ref())
}
