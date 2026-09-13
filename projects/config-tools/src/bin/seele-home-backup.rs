use std::os::unix::process::CommandExt;
fn main() -> seele_config_tools::Result {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.len() != 1 {
        return Err("usage: seele-home-backup PATH".into());
    }
    let mut destination = arguments[0].clone();
    destination.push(".bak");
    let executable = std::env::var_os("SEELE_MV").ok_or("missing packaged backup executable")?;
    let error = std::process::Command::new(executable)
        .args(["--backup=numbered", "--no-target-directory", "--"])
        .arg(&arguments[0])
        .arg(destination)
        .exec();
    Err(error.into())
}
