//! Bounded firmware-update notification; never installs an update.
use seele_runtime::{fs, process, Result};
use serde_json::Value;
use std::{io, path::Path, process::Command, time::Duration};

const LIMIT: usize = 1024 * 1024;
const SUMMARY: &str = "Firmware updates are available";

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid firmware update report")
}
fn label(value: Option<&Value>, fallback: &str) -> io::Result<String> {
    let text = match value {
        None | Some(Value::Null) => fallback,
        Some(Value::String(text)) => text,
        _ => return Err(invalid()),
    };
    Ok(text
        .chars()
        .filter(|c| !c.is_control() && seele_runtime::redact::visible(*c))
        .take(160)
        .collect())
}
fn pending(report: &[u8]) -> io::Result<String> {
    let report: Value = serde_json::from_slice(report).map_err(|_| invalid())?;
    let object = report.as_object().ok_or_else(invalid)?;
    let devices = match object.get("Devices") {
        None | Some(Value::Null) => return Ok(String::new()),
        Some(Value::Array(devices)) if devices.len() <= 256 => devices,
        _ => return Err(invalid()),
    };
    let mut lines = Vec::new();
    for device in devices {
        let device = device.as_object().ok_or_else(invalid)?;
        let releases = match device.get("Releases") {
            None | Some(Value::Null) => continue,
            Some(Value::Array(releases)) => releases,
            _ => return Err(invalid()),
        };
        let Some(release) = releases.first() else {
            continue;
        };
        let release = release.as_object().ok_or_else(invalid)?;
        lines.push(format!(
            "{} {} → {}",
            label(device.get("Name"), "Unknown device")?,
            label(device.get("Version"), "?")?,
            label(release.get("Version"), "?")?
        ));
    }
    lines.sort();
    lines.dedup();
    let text = lines.join(", ");
    if text.len() > 16 * 1024 {
        return Err(invalid());
    }
    Ok(text)
}
fn announce(text: &str, cancel: &dyn seele_runtime::cancel::Cancellation) -> Result {
    let status = process::discard(
        Command::new("dbus-send").args([
            "--system",
            "/",
            "net.nuetzlich.SystemNotifications.Notify",
            &format!("string:{SUMMARY}"),
            &format!("string:{text}. Install with: fwupdmgr update"),
        ]),
        b"",
        process::Limits {
            timeout: Duration::from_secs(10),
            output: 0,
        },
        cancel,
    )?;
    if !status.success() {
        return Err(io::Error::other("firmware notification failed").into());
    }
    Ok(())
}
fn publish_pending(state: &Path, text: &str, notify: impl FnOnce(&str) -> Result) -> Result {
    if text.is_empty() {
        match std::fs::remove_file(state) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        return Ok(());
    }
    match fs::read_private(state, 16 * 1024) {
        Ok(old) if old == text.as_bytes() => return Ok(()),
        Ok(_) => (),
        Err(e) if e.kind() == io::ErrorKind::NotFound => (),
        Err(e) => return Err(e.into()),
    }
    notify(text)?;
    fs::atomic_write(state, text.as_bytes())?;
    Ok(())
}
pub fn run(arguments: &[String]) -> Result {
    let cancel = process::termination_signal()?;
    if arguments == ["--test"] {
        return announce("Seele firmware delivery test 0.0 → 0.1", &cancel);
    }
    if !arguments.is_empty() {
        return Err(io::Error::other("usage: seele-firmware-check [--test]").into());
    }
    let directory = Path::new("/run/seele-firmware-check");
    fs::private_directory(directory)?;
    let output = process::capture(
        Command::new("fwupdmgr").args(["get-updates", "--json"]),
        b"",
        process::Limits {
            timeout: Duration::from_secs(120),
            output: LIMIT,
        },
        &cancel,
    )?;
    if !output.status.success() {
        return Err(io::Error::other("firmware update query failed").into());
    }
    let text = pending(&output.stdout)?;
    publish_pending(&directory.join("announced"), &text, |text| {
        announce(text, &cancel)
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_and_sanitizes_vendor_metadata() {
        assert_eq!(pending(br#"{}"#).unwrap(), "");
        assert_eq!(pending(br#"{"Devices":[{"Name":"unused"}]}"#).unwrap(), "");
        assert_eq!(pending(br#"{"Devices":[{"Name":"Dock\n\u202e","Version":"1","Releases":[{"Version":"2"}]}]}"#).unwrap(), "Dock 1 → 2");
        for bad in [
            br#"[]"#.as_slice(),
            br#"{"Devices":{}}"#,
            br#"{"Devices":[{"Releases":1}]}"#,
            br#"{"Devices":[{"Name":4,"Releases":[{}]}]}"#,
        ] {
            assert!(pending(bad).is_err());
        }
    }
    #[test]
    fn deduplicates_only_successful_delivery_and_rearms_after_no_updates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("announced");
        assert!(publish_pending(&path, "Dock 1 → 2", |_| Err(
            io::Error::other("failed").into()
        ))
        .is_err());
        assert!(!path.exists());
        publish_pending(&path, "Dock 1 → 2", |_| Ok(())).unwrap();
        publish_pending(&path, "Dock 1 → 2", |_| panic!("duplicate")).unwrap();
        publish_pending(&path, "", |_| panic!("empty")).unwrap();
        assert!(!path.exists());
        publish_pending(&path, "Dock 1 → 2", |_| Ok(())).unwrap();
    }
}
