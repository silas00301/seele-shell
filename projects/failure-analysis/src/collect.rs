use crate::{
    executable, run,
    text::{bounded, clean, clean_value, timestamp},
    MAX_REPORT,
};
use regex::Regex;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::sync::LazyLock;

pub const PROPERTIES: [&str; 13] = [
    "Id",
    "Description",
    "LoadState",
    "ActiveState",
    "SubState",
    "Result",
    "ExecMainCode",
    "ExecMainStatus",
    "ExecStart",
    "InvocationID",
    "FragmentPath",
    "SourcePath",
    "StateChangeTimestamp",
];
static UNIT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.:@\\-]{1,256}\.service$").unwrap());
static DERIVATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"/nix/store/[0-9a-z]{32}-[A-Za-z0-9+._?=-]+\.drv").unwrap());
static KERNEL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:kernel|oom(?:-kill(?:er)?)?|out of memory|segfault|general protection|i/o error|nvme|nvidia|amdgpu|drm|firmware|device reset)\b").unwrap()
});
pub fn valid_unit(name: &str) -> bool {
    UNIT.is_match(name) && !name.starts_with("seele-failure-report@")
}
fn valid_invocation(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit())
}
pub fn parse_properties(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| PROPERTIES.contains(name))
        .map(|(name, value)| (name.to_owned(), clean(value, 8000)))
        .collect()
}
pub fn parse_journal(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|v| v.is_object() && v.get("MESSAGE").is_some())
        .take(120)
        .collect()
}
pub fn journal_timestamp(entry: &Value) -> Option<i64> {
    entry["__REALTIME_TIMESTAMP"]
        .as_str()
        .and_then(|v| v.parse().ok())
        .or_else(|| entry["__REALTIME_TIMESTAMP"].as_i64())
}
pub fn format_journal(entries: &[Value], limit: usize) -> String {
    let lines: Vec<_> = entries
        .iter()
        .map(|entry| {
            let identifier = clean_value(
                entry
                    .get("SYSLOG_IDENTIFIER")
                    .filter(|v| !v.is_null())
                    .or_else(|| entry.get("_COMM"))
                    .unwrap_or(&Value::String("unit".into())),
                80,
            );
            let pid = entry
                .get("_PID")
                .map(|v| clean_value(v, 24))
                .unwrap_or_default();
            let origin = if pid.is_empty() {
                identifier
            } else {
                format!("{identifier}[{pid}]")
            };
            let message = clean_value(&entry["MESSAGE"], 2000).replace('\n', " ⏎ ");
            let time = journal_timestamp(entry)
                .map(timestamp)
                .unwrap_or_else(|| "unknown-time".into());
            format!("{time} {origin}: {message}")
        })
        .collect();
    bounded(&lines.join("\n"), limit, true)
}
fn messages(entries: &[Value]) -> String {
    entries
        .iter()
        .map(|v| clean_value(&v["MESSAGE"], 4000))
        .collect::<Vec<_>>()
        .join("\n")
}
fn nix_logs(entries: &[Value], cancel: &AtomicUsize) -> Vec<(String, String)> {
    let nix = std::env::var_os("SEELE_FAILURE_NIX").or_else(|| {
        [
            "/run/current-system/sw/bin/nix",
            "/nix/var/nix/profiles/default/bin/nix",
        ]
        .into_iter()
        .find(|p| std::fs::metadata(p).is_ok())
        .map(Into::into)
    });
    let Some(nix) = nix else {
        return vec![];
    };
    let mut seen = HashSet::new();
    DERIVATION
        .find_iter(&messages(entries))
        .map(|v| v.as_str())
        .filter(|v| seen.insert((*v).to_owned()))
        .take(3)
        .filter_map(|derivation| {
            let output = run(
                Command::new(&nix).args(["--offline", "log", derivation]),
                b"",
                20,
                cancel,
            );
            let text = if output.stdout.is_empty() {
                output.stderr
            } else {
                output.stdout
            };
            if text.is_empty() {
                None
            } else {
                Some((
                    derivation.to_owned(),
                    clean(&bounded(&text, 12 * 1024, true), 14 * 1024),
                ))
            }
        })
        .collect()
}
pub fn kernel_bounds(entries: &[Value]) -> Option<(i64, i64)> {
    if !KERNEL.is_match(&messages(entries)) {
        return None;
    }
    let timestamps: Vec<_> = entries.iter().filter_map(journal_timestamp).collect();
    let start = timestamps
        .iter()
        .min()?
        .div_euclid(1_000_000)
        .checked_sub(1)?;
    let end = timestamps
        .iter()
        .max()?
        .checked_add(999_999)?
        .div_euclid(1_000_000)
        .checked_add(1)?;
    Some((start, end))
}
fn kernel_entries(entries: &[Value], cancel: &AtomicUsize) -> Vec<Value> {
    let Some((since, until)) = kernel_bounds(entries) else {
        return vec![];
    };
    let output = run(
        Command::new(executable("SEELE_FAILURE_JOURNALCTL", "journalctl"))
            .args([
                "--no-pager",
                "--output=json",
                "--dmesg",
                "--priority=warning..alert",
                "--lines=40",
            ])
            .arg(format!("--since=@{since}"))
            .arg(format!("--until=@{until}")),
        b"",
        10,
        cancel,
    );
    parse_journal(&output.stdout)
}
pub fn collect_unit(unit: &str, cancel: &AtomicUsize) -> std::io::Result<(String, String)> {
    if !valid_unit(unit) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    let mut command = Command::new(executable("SEELE_FAILURE_SYSTEMCTL", "systemctl"));
    command
        .args(["show", "--no-pager"])
        .args(PROPERTIES.map(|name| format!("--property={name}")))
        .arg(unit);
    let output = run(&mut command, b"", 10, cancel);
    let mut properties = parse_properties(&output.stdout);
    let invocation = std::env::var("MONITOR_INVOCATION_ID").unwrap_or_default();
    if std::env::var("MONITOR_UNIT").is_ok_and(|v| v == unit) && valid_invocation(&invocation) {
        properties.insert("InvocationID".into(), invocation.to_ascii_lowercase());
        for (env, property) in [
            ("MONITOR_SERVICE_RESULT", "Result"),
            ("MONITOR_EXIT_CODE", "ExecMainCode"),
            ("MONITOR_EXIT_STATUS", "ExecMainStatus"),
        ] {
            if let Ok(value) = std::env::var(env) {
                if !value.is_empty() {
                    properties.insert(property.into(), clean(&value, 100));
                }
            }
        }
    }
    let invocation = properties.get("InvocationID").cloned().unwrap_or_default();
    let entries = if valid_invocation(&invocation) {
        let output = run(
            Command::new(executable("SEELE_FAILURE_JOURNALCTL", "journalctl"))
                .args(["--no-pager", "--output=json", "--lines=80"])
                .arg(format!(
                    "_SYSTEMD_INVOCATION_ID={}",
                    invocation.to_ascii_lowercase()
                ))
                .arg("+")
                .arg(format!("INVOCATION_ID={}", invocation.to_ascii_lowercase())),
            b"",
            15,
            cancel,
        );
        parse_journal(&output.stdout)
    } else {
        vec![]
    };
    let mut sections = vec![
        "Seele failure report".into(),
        format!("Collected: {}", seele_runtime::time::timestamp()),
        format!("Failed unit: {unit}"),
        String::new(),
        "[Unit state]".into(),
    ];
    if properties.is_empty() {
        sections.push(clean(
            if output.stderr.is_empty() {
                "systemctl returned no unit state"
            } else {
                &output.stderr
            },
            4000,
        ));
    } else {
        for name in PROPERTIES {
            if let Some(value) = properties.get(name) {
                sections.push(format!("{name}={value}"));
            }
        }
    }
    sections.extend([
        String::new(),
        "[Journal for failed invocation]".into(),
        if entries.is_empty() {
            "No journal entries were available for this invocation.".into()
        } else {
            format_journal(&entries, 48 * 1024)
        },
    ]);
    for (derivation, log) in nix_logs(&entries, cancel) {
        sections.extend([
            String::new(),
            format!("[Local Nix build log: {derivation}]"),
            log,
        ]);
    }
    let kernel = kernel_entries(&entries, cancel);
    if !kernel.is_empty() {
        sections.extend([
            String::new(),
            "[Kernel warnings during failed invocation]".into(),
            format_journal(&kernel, 16 * 1024),
        ]);
    }
    let mut summary = format!(
        "Result: {}",
        properties
            .get("Result")
            .or_else(|| properties.get("SubState"))
            .map_or("failed", String::as_str)
    );
    if let Some(status) = properties.get("ExecMainStatus") {
        summary.push_str(&format!(" · exit {status}"));
    }
    if let Some(message) = entries
        .iter()
        .rev()
        .map(|v| clean_value(&v["MESSAGE"], 300).replace('\n', " "))
        .find(|v| !v.is_empty())
    {
        summary.push('\n');
        summary.push_str(&message);
    }
    Ok((
        bounded(
            &(sections.join("\n").trim_end().to_owned() + "\n"),
            MAX_REPORT,
            false,
        ),
        bounded(&summary, 420, false),
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn invocation_and_kernel_boundaries() {
        assert!(valid_unit("seele-failure-test.service"));
        assert!(!valid_unit("seele-failure-report@demo.service.service"));
        assert!(!valid_unit("bad\n.service"));
        assert!(
            kernel_bounds(&[json!({"MESSAGE":"plain failure","__REALTIME_TIMESTAMP":"2"})])
                .is_none()
        );
        assert_eq!(
            kernel_bounds(&[
                json!({"MESSAGE":"GPU device reset","__REALTIME_TIMESTAMP":"2000000"}),
                json!({"MESSAGE":"failure","__REALTIME_TIMESTAMP":"4000000"})
            ]),
            Some((1, 5))
        );
        assert_eq!(
            parse_journal("{}\ninvalid\n{\"MESSAGE\":[65,66]}\n").len(),
            1
        );
    }
}
