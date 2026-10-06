use std::os::unix::fs::PermissionsExt;
use std::process::Command;

#[test]
fn wallet_lookup_stays_in_the_helper_process_group() {
    let directory = tempfile::tempdir().unwrap();
    let tool = directory.path().join("secret-tool");
    let record = directory.path().join("group");
    std::fs::write(
        &tool,
        format!(
            "#!/bin/sh\npgid=$(ps -o pgid= -p $$)\nparent=$(ps -o pgid= -p \"$PPID\")\nprintf '%s %s\\n' \"$pgid\" \"$parent\" > {}\nprintf 'SYNTHETIC-KEY\\n'\n",
            shell_quote(&record)
        ),
    )
    .unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o700)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_seele-claude-key"))
        .env_clear()
        .env(
            "PATH",
            std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()),
        )
        .env("SEELE_BROKER_SECRET_TOOL", &tool)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = std::fs::read_to_string(record).unwrap();
    let mut groups = text.split_whitespace();
    let child: i32 = groups.next().unwrap().parse().unwrap();
    let parent: i32 = groups.next().unwrap().parse().unwrap();
    assert_eq!(
        child, parent,
        "lookup left the helper process group: {text}"
    );
}

fn shell_quote(path: &std::path::Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}
