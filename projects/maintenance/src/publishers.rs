//! Complete source snapshots: a probe failure is never recovery. Each source
//! runs independently; every external process has a byte and time budget.
use crate::{
    args,
    model::{valid_key, Finding, Lifecycle, Result, Urgency},
    Executor,
};
use regex::Regex;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    ffi::CString,
    path::Path,
    sync::LazyLock,
    time::{Duration, UNIX_EPOCH},
};

static UNIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9_][A-Za-z0-9_.:@\\-]*\.(?:service|socket|target|device|mount|automount|swap|timer|path|slice|scope)$").unwrap()
});
static REF: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+(?:/[A-Za-z0-9_.-]+)*$").unwrap());
pub fn valid_unit(value: &str) -> bool {
    value.len() <= 200 && UNIT.is_match(value)
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .filter(|s| s.len() <= 4096 && !s.contains('\0'))
        .ok_or("invalid_configuration")
}
fn numeric(value: &Value, key: &str, default: f64) -> Result<f64> {
    if value[key].is_null() {
        return Ok(default);
    }
    value[key]
        .as_f64()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .ok_or("invalid_configuration")
}
fn items(cfg: &Value) -> Result<&[Value]> {
    if cfg["items"].is_null() {
        return Ok(&[]);
    }
    cfg["items"]
        .as_array()
        .filter(|v| v.len() <= 512)
        .map(Vec::as_slice)
        .ok_or("invalid_configuration")
}
fn scope(value: Option<&str>, default: &str) -> Result<&'static str> {
    match value.unwrap_or(default) {
        "user" => Ok("--user"),
        "system" => Ok("--system"),
        _ => Err("invalid_service_scope"),
    }
}
fn report(
    key: &str,
    title: String,
    explanation: &str,
    details: String,
    urgency: Urgency,
    actions: &[&str],
) -> Result<Finding> {
    if !valid_key(key) {
        return Err("invalid_source_identity");
    }
    Ok(Finding {
        key: key.into(),
        title,
        explanation: explanation.into(),
        diagnostic: Some(details.chars().take(4096).collect()),
        details,
        urgency,
        lifecycle: Lifecycle::Ongoing,
        actions: args(actions),
    })
}
fn checked(executor: &dyn Executor, arguments: &[String], timeout: u64) -> Result<String> {
    let (code, text) = executor.run(arguments, b"", Duration::from_secs(timeout))?;
    if code != 0 {
        return Err("probe_failed");
    }
    Ok(text)
}
fn json_output(executor: &dyn Executor, arguments: &[String], timeout: u64) -> Result<Value> {
    serde_json::from_str(&checked(executor, arguments, timeout)?)
        .map_err(|_| "invalid_probe_snapshot")
}
pub fn validate_config(config: &Value) -> Result<()> {
    if !config.is_object() {
        return Err("invalid_configuration");
    }
    for source in crate::model::SOURCES {
        let cfg = &config[source];
        if !cfg.is_null() && !cfg.is_object() {
            return Err("invalid_configuration");
        }
        if !cfg["enabled"].is_null() && !cfg["enabled"].is_boolean() {
            return Err("invalid_configuration");
        }
        let mut seen = HashSet::new();
        for item in items(cfg)? {
            let id = string(item, "id")?;
            if !valid_key(id) || !seen.insert(id) {
                return Err("invalid_source_identity");
            }
        }
        if let Some(paths) = cfg["paths"].as_array() {
            if paths.len() > 512 {
                return Err("invalid_configuration");
            }
        }
        for interval in [&cfg["intervalSeconds"], &config["intervalSeconds"]] {
            if !interval.is_null() && interval.as_u64().is_none_or(|n| n > 365 * 86400) {
                return Err("invalid_interval");
            }
        }
    }
    Ok(())
}
pub fn collect(
    config: &Value,
    source: &str,
    executor: &dyn Executor,
    now: f64,
) -> Result<Vec<Finding>> {
    let cfg = &config[source];
    if cfg["enabled"] == false {
        return Ok(vec![]);
    }
    match source {
        "systemd" => systemd(cfg, executor),
        "backups" => backups(cfg, executor, now),
        "disk" => disk(cfg),
        "flake" => flake(cfg, executor),
        "certificates" => certificates(cfg, executor, now),
        "inputs" => inputs(cfg, executor, now),
        _ => Err("unknown_source"),
    }
}
fn systemd(cfg: &Value, executor: &dyn Executor) -> Result<Vec<Finding>> {
    let selected = scope(cfg["scope"].as_str(), "system")?;
    let data = json_output(
        executor,
        &args(&[
            "systemctl",
            selected,
            "list-units",
            "--all",
            "--state=failed",
            "--no-pager",
            "--json=short",
        ]),
        30,
    )?;
    let units = data
        .as_array()
        .filter(|v| v.len() <= 512)
        .ok_or("invalid_service_snapshot")?;
    units
        .iter()
        .map(|unit| {
            let name = string(unit, "unit")?;
            if !valid_unit(name) {
                return Err("invalid_service_identity");
            }
            report(
                &format!("{}/{}", &selected[2..], name),
                "Failed system service".into(),
                "A systemd unit is in the failed state.",
                format!("Unit: {name}\nState: failed"),
                Urgency::Soon,
                &["recheck", "open-logs"],
            )
        })
        .collect()
}
pub fn backup_report(
    item: &Value,
    text: &str,
    marker: Option<f64>,
    now: f64,
) -> Result<Option<Finding>> {
    let unit = string(item, "unit")?;
    let label = string(item, "label")?;
    let id = string(item, "id")?;
    let state: BTreeMap<_, _> = text
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect();
    for key in [
        "LoadState",
        "ActiveState",
        "Result",
        "ExecMainStatus",
        "ExecMainExitTimestamp",
    ] {
        if !state.contains_key(key) {
            return Err("invalid_backup_snapshot");
        }
    }
    if state["LoadState"] != "loaded" {
        return Err("backup_unit_unavailable");
    }
    if state["ActiveState"] == "failed"
        || !["success", ""].contains(&state["Result"])
        || state["ExecMainStatus"] != "0"
    {
        return report(
            id,
            format!("{label}: backup failed"),
            "The configured backup service did not finish successfully.",
            format!("Unit: {unit}\nLatest service outcome: failed"),
            Urgency::Soon,
            &["recheck", "open-logs", "retry"],
        )
        .map(Some);
    }
    let timestamp = if item["successFile"].as_str().is_some_and(|s| !s.is_empty()) {
        marker.unwrap_or(0.0)
    } else {
        let value = state["ExecMainExitTimestamp"]
            .strip_prefix('@')
            .unwrap_or(state["ExecMainExitTimestamp"]);
        if value.is_empty() || value == "n/a" {
            0.0
        } else {
            if !value.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
                return Err("invalid_backup_timestamp");
            }
            value
                .parse::<f64>()
                .map_err(|_| "invalid_backup_timestamp")?
        }
    };
    if !timestamp.is_finite() || timestamp < 0.0 || timestamp > now + 300.0 {
        return Err("invalid_backup_timestamp");
    }
    let max_age = numeric(item, "maxAgeHours", 24.0)?;
    if max_age == 0.0 {
        return Err("invalid_configuration");
    }
    if timestamp == 0.0 || now - timestamp > max_age * 3600.0 {
        let detail = if timestamp == 0.0 {
            "No recorded successful completion.".into()
        } else {
            format!("Last successful completion: {}", iso(timestamp)?)
        };
        return report(
            id,
            format!("{label}: backup overdue"),
            "No sufficiently recent successful backup is recorded.",
            format!("Unit: {unit}\n{detail}"),
            Urgency::Soon,
            &["recheck", "open-logs", "retry"],
        )
        .map(Some);
    }
    Ok(None)
}
fn backups(cfg: &Value, executor: &dyn Executor, now: f64) -> Result<Vec<Finding>> {
    let mut rows = vec![];
    for item in items(cfg)? {
        let unit = string(item, "unit")?;
        if !valid_unit(unit) || !unit.ends_with(".service") {
            return Err("invalid_backup_unit");
        }
        let text = checked(
            executor,
            &args(&[
                "systemctl",
                scope(item["scope"].as_str(), "user")?,
                "show",
                "--timestamp=unix",
                "--property=LoadState,ActiveState,Result,ExecMainStatus,ExecMainExitTimestamp",
                "--",
                unit,
            ]),
            30,
        )?;
        let marker = if let Some(path) = item["successFile"].as_str().filter(|p| !p.is_empty()) {
            match std::fs::metadata(path) {
                Ok(m) => Some(
                    m.modified()
                        .map_err(|_| "probe_unavailable")?
                        .duration_since(UNIX_EPOCH)
                        .map_err(|_| "invalid_backup_timestamp")?
                        .as_secs_f64(),
                ),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err("probe_unavailable"),
            }
        } else {
            None
        };
        if let Some(row) = backup_report(item, &text, marker, now)? {
            rows.push(row);
        }
    }
    Ok(rows)
}
pub fn disk_report(
    path: &str,
    blocks: u64,
    available: u64,
    files: u64,
    inodes: u64,
    soon: f64,
    now: f64,
) -> Result<Option<Finding>> {
    if !(0.0 < soon && soon < now && now <= 100.0)
        || blocks == 0
        || available > blocks
        || inodes > files
    {
        return Err("invalid_disk_capacity");
    }
    let percent = (1.0 - available as f64 / blocks as f64) * 100.0;
    let inode_percent = if files > 0 {
        (1.0 - inodes as f64 / files as f64) * 100.0
    } else {
        0.0
    };
    let severity = percent.max(inode_percent);
    if severity < soon {
        return Ok(None);
    }
    let key = format!("{:x}", Sha256::digest(path.as_bytes()));
    let detail = if percent >= now {
        "Available space below critical threshold."
    } else if percent >= soon {
        "Available space below warning threshold."
    } else {
        "Available space within threshold."
    };
    let inodes = if inode_percent >= now {
        "\nAvailable inodes below critical threshold."
    } else if inode_percent >= soon {
        "\nAvailable inodes below warning threshold."
    } else {
        ""
    };
    report(
        &key[..16],
        "Disk space pressure".into(),
        "Free filesystem capacity is below the configured threshold.",
        format!("Filesystem: {path}\n{detail}{inodes}"),
        if severity >= now {
            Urgency::Now
        } else {
            Urgency::Soon
        },
        &["recheck"],
    )
    .map(Some)
}
fn disk(cfg: &Value) -> Result<Vec<Finding>> {
    let soon = numeric(cfg, "soonPercent", 85.0)?;
    let now = numeric(cfg, "nowPercent", 95.0)?;
    if !(0.0 < soon && soon < now && now <= 100.0) {
        return Err("invalid_disk_thresholds");
    }
    let default = vec![Value::String("/".into())];
    let paths = if cfg["paths"].is_null() {
        &default
    } else {
        cfg["paths"]
            .as_array()
            .filter(|p| p.len() <= 512)
            .ok_or("invalid_configuration")?
    };
    let mut rows = vec![];
    for path in paths {
        let path = path
            .as_str()
            .filter(|p| p.len() <= 4096)
            .ok_or("invalid_configuration")?;
        let name = CString::new(path).map_err(|_| "invalid_configuration")?;
        let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(name.as_ptr(), &mut stat) } != 0 {
            return Err("probe_unavailable");
        }
        if let Some(row) = disk_report(
            path,
            stat.f_blocks,
            stat.f_bavail,
            stat.f_files,
            stat.f_favail,
            soon,
            now,
        )? {
            rows.push(row);
        }
    }
    Ok(rows)
}
fn flake(cfg: &Value, executor: &dyn Executor) -> Result<Vec<Finding>> {
    let path = string(cfg, "path")?;
    if !Path::new(path).join("flake.nix").is_file() {
        return Err("flake_unavailable");
    }
    let (code, _) = executor.run(
        &args(&[
            "nix",
            "flake",
            "check",
            "--no-build",
            "--no-write-lock-file",
            "--",
            path,
        ]),
        b"",
        Duration::from_secs(900),
    )?;
    if code == 0 {
        return Ok(vec![]);
    }
    Ok(vec![report(
        "check",
        "Flake checks failed".into(),
        "The configured flake failed its evaluation checks.",
        format!("Command: nix flake check --no-build --no-write-lock-file\nExit status: {code}"),
        Urgency::Soon,
        &["recheck"],
    )?])
}
fn iso(timestamp: f64) -> Result<String> {
    if !timestamp.is_finite() || timestamp < 0.0 || timestamp > i64::MAX as f64 {
        return Err("invalid_timestamp");
    }
    seele_runtime::time::format_timestamp(timestamp as libc::time_t)
        .map(|s| s.replace('Z', "+00:00"))
        .ok_or("invalid_timestamp")
}
pub fn expiry(text: &str) -> Result<f64> {
    let text = text
        .trim()
        .strip_prefix("notAfter=")
        .ok_or("invalid_certificate")?;
    let parts: Vec<_> = text.split_whitespace().collect();
    if parts.len() != 5 || parts[4] != "GMT" {
        return Err("invalid_certificate");
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| *m == parts[0])
    .ok_or("invalid_certificate")?;
    let day = parts[1].parse::<i32>().map_err(|_| "invalid_certificate")?;
    let year = parts[3].parse::<i32>().map_err(|_| "invalid_certificate")?;
    let clock: Vec<_> = parts[2]
        .split(':')
        .map(str::parse::<i32>)
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| "invalid_certificate")?;
    if !(1..=31).contains(&day)
        || !(1970..=9999).contains(&year)
        || clock.len() != 3
        || !(0..24).contains(&clock[0])
        || !(0..60).contains(&clock[1])
        || !(0..60).contains(&clock[2])
    {
        return Err("invalid_certificate");
    }
    let mut value: libc::tm = unsafe { std::mem::zeroed() };
    value.tm_year = year - 1900;
    value.tm_mon = month as i32;
    value.tm_mday = day;
    value.tm_hour = clock[0];
    value.tm_min = clock[1];
    value.tm_sec = clock[2];
    let stamp = unsafe { libc::timegm(&mut value) };
    if stamp < 0 || value.tm_mday != day || value.tm_mon != month as i32 {
        return Err("invalid_certificate");
    }
    Ok(stamp as f64)
}
fn certificates(cfg: &Value, executor: &dyn Executor, now: f64) -> Result<Vec<Finding>> {
    let mut rows = vec![];
    for item in items(cfg)? {
        let critical = numeric(item, "nowDays", 7.0)?;
        let warning = numeric(item, "soonDays", 30.0)?;
        if critical >= warning {
            return Err("invalid_certificate_thresholds");
        }
        let text = checked(
            executor,
            &args(&[
                "openssl",
                "x509",
                "-in",
                string(item, "path")?,
                "-noout",
                "-enddate",
            ]),
            30,
        )?;
        let expires = expiry(&text)?;
        let remaining = (expires - now) / 86400.0;
        if remaining <= warning {
            rows.push(report(
                string(item, "id")?,
                format!("{}: certificate expiring", string(item, "label")?),
                "A configured certificate is nearing or past its expiration.",
                format!("Certificate expires: {}", iso(expires)?),
                if remaining <= critical {
                    Urgency::Now
                } else {
                    Urgency::Soon
                },
                &["recheck"],
            )?);
        }
    }
    Ok(rows)
}
pub fn original_ref(original: &Value) -> Result<String> {
    let kind = original["type"]
        .as_str()
        .filter(|k| ["github", "gitlab"].contains(k))
        .ok_or("unsupported_input_reference")?;
    if original["dir"].as_str().is_some_and(|s| !s.is_empty())
        || original["host"].as_str().is_some_and(|s| !s.is_empty())
    {
        return Err("unsupported_input_reference");
    }
    let mut pieces = vec![string(original, "owner")?, string(original, "repo")?];
    if let Some(reference) = original["rev"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| original["ref"].as_str().filter(|s| !s.is_empty()))
    {
        pieces.push(reference);
    }
    if pieces
        .iter()
        .any(|p| !REF.is_match(p) || p.split('/').any(|part| [".", ".."].contains(&part)))
    {
        return Err("unsupported_input_reference");
    }
    Ok(format!("{kind}:{}", pieces.join("/")))
}
fn revision(value: &Value) -> Result<&str> {
    value
        .as_str()
        .filter(|v| (7..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or("input_revision_unavailable")
}
fn inputs(cfg: &Value, executor: &dyn Executor, now: f64) -> Result<Vec<Finding>> {
    let bytes = crate::read_bounded(
        &Path::new(string(cfg, "path")?).join("flake.lock"),
        4 * 1024 * 1024,
        false,
    )
    .map_err(|_| "lock_unavailable")?;
    let lock: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid_lock")?;
    let nodes = lock["nodes"].as_object().ok_or("invalid_lock")?;
    let root = nodes.get(string(&lock, "root")?).ok_or("invalid_lock")?;
    let mut rows = vec![];
    for item in items(cfg)? {
        let name = string(item, "id")?;
        let target = root["inputs"][name]
            .as_str()
            .ok_or("unsupported_input_follow")?;
        let node = nodes.get(target).ok_or("invalid_lock")?;
        let pinned = &node["locked"];
        let timestamp = pinned["lastModified"]
            .as_f64()
            .filter(|t| t.is_finite() && *t > 0.0 && *t <= now + 300.0)
            .ok_or("input_age_unavailable")?;
        let age = numeric(item, "maxAgeDays", 30.0)?;
        if age == 0.0 {
            return Err("invalid_configuration");
        }
        if now - timestamp <= age * 86400.0 {
            continue;
        }
        let reference = original_ref(&node["original"])?;
        let metadata = json_output(
            executor,
            &args(&[
                "nix",
                "flake",
                "metadata",
                "--json",
                "--no-write-lock-file",
                "--refresh",
                "--",
                &reference,
            ]),
            180,
        )?;
        let latest = revision(if metadata["locked"]["rev"].is_null() {
            &metadata["revision"]
        } else {
            &metadata["locked"]["rev"]
        })?;
        let current = revision(&pinned["rev"])?;
        if latest != current {
            rows.push(report(name,format!("{name}: critical input outdated"),"A configured critical input exceeds its age threshold and a newer upstream revision is available.",format!("Input: {name}\nPinned revision: {current}\nAvailable revision: {latest}"),Urgency::Eventually,&["recheck"])?);
        }
    }
    Ok(rows)
}
