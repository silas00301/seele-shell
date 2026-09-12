use seele_failure_analysis::collect::collect_unit;
use std::{fs, os::unix::fs::PermissionsExt, sync::atomic::AtomicUsize};
fn script(path: impl AsRef<std::path::Path>, source: &str) -> std::io::Result<()> {
    let shell = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("sh"))
        .find(|path| path.is_file())
        .expect("fixture needs sh in PATH");
    fs::write(
        path,
        source.replacen("#!/bin/sh", &format!("#!{}", shell.display()), 1),
    )
}
#[test]
fn invocation_collection_restart_race_nix_kernel_and_missing_identity() {
    let root = tempfile::tempdir().unwrap();
    let tools = root.path();
    let systemctl = tools.join("systemctl");
    let journal = tools.join("journalctl");
    let nix = tools.join("nix");
    script(&systemctl,"#!/bin/sh\nprintf '%s\\n' 'Id=demo.service' 'Result=exit-code' 'ExecMainStatus=7' 'InvocationID=0123456789abcdef0123456789abcdef'\n").unwrap();
    script(&journal,r#"#!/bin/sh
printf '%s\n' "$@" >> "$SEELE_FIXTURE_LOG"
case " $* " in
 *' --dmesg '*) printf '%s\n' '{"__REALTIME_TIMESTAMP":"3000000","SYSLOG_IDENTIFIER":"kernel","MESSAGE":"GPU reset reported by driver"}';;
 *) printf '%s\n' '{"__REALTIME_TIMESTAMP":"2000000","SYSLOG_IDENTIFIER":"demo","_PID":"10","MESSAGE":"building /nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-demo.drv"}' '{"__REALTIME_TIMESTAMP":"4000000","SYSLOG_IDENTIFIER":"demo","_PID":"10","MESSAGE":"GPU device reset while starting"}';;
esac
"#).unwrap();
    script(&nix,"#!/bin/sh\nprintf '%s\\n' \"$@\" >> \"$SEELE_FIXTURE_LOG\"\nprintf '%s\\n' 'builder failed at the final step'\n").unwrap();
    for path in [&systemctl, &journal, &nix] {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let log = tools.join("calls");
    std::env::set_var("SEELE_FIXTURE_LOG", &log);
    std::env::set_var("SEELE_FAILURE_SYSTEMCTL", systemctl);
    std::env::set_var("SEELE_FAILURE_JOURNALCTL", journal);
    std::env::set_var("SEELE_FAILURE_NIX", nix);
    std::env::remove_var("MONITOR_UNIT");
    std::env::remove_var("MONITOR_INVOCATION_ID");
    let cancel = AtomicUsize::new(0);
    let (report, summary) = collect_unit("demo.service", &cancel).unwrap();
    let calls = fs::read_to_string(&log).unwrap();
    for expected in [
        "_SYSTEMD_INVOCATION_ID=0123456789abcdef0123456789abcdef",
        "INVOCATION_ID=0123456789abcdef0123456789abcdef",
        "--offline\nlog\n/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-demo.drv",
        "--since=@1",
        "--until=@5",
    ] {
        assert!(calls.contains(expected), "{expected}");
    }
    assert!(report.contains("builder failed at the final step"));
    assert!(report.contains("GPU reset reported by driver"));
    assert!(summary.contains("exit 7"));
    std::env::set_var("MONITOR_UNIT", "demo.service");
    std::env::set_var("MONITOR_INVOCATION_ID", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    std::env::set_var("MONITOR_SERVICE_RESULT", "exit-code");
    std::env::set_var("MONITOR_EXIT_STATUS", "23");
    fs::write(&log, "").unwrap();
    let (report, summary) = collect_unit("demo.service", &cancel).unwrap();
    assert!(report.contains("InvocationID=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    assert!(summary.contains("exit 23"));
    assert!(fs::read_to_string(&log)
        .unwrap()
        .contains("_SYSTEMD_INVOCATION_ID=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
    std::env::remove_var("MONITOR_UNIT");
    std::env::remove_var("MONITOR_INVOCATION_ID");
    script(
        tools.join("systemctl"),
        "#!/bin/sh\nprintf '%s\\n' 'Id=demo.service' 'InvocationID='\n",
    )
    .unwrap();
    fs::write(&log, "").unwrap();
    let (report, _) = collect_unit("demo.service", &cancel).unwrap();
    assert!(report.contains("No journal entries were available"));
    assert_eq!(fs::read_to_string(&log).unwrap(), "");
    assert!(collect_unit("seele-failure-test.service", &cancel).is_ok());
    assert!(collect_unit("seele-failure-report@demo.service.service", &cancel).is_err());
}
