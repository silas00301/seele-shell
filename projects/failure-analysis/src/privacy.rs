//! Environment-specific identities augment the shared syntax redactor only
//! immediately before explicit inference or sanitized notification summaries.
use crate::{executable, run};
use std::collections::HashSet;
use std::io::Read;
use std::process::Command;
use std::sync::atomic::AtomicUsize;
pub fn redact(report: &str, cancel: &AtomicUsize) -> String {
    let mut identifiers: HashSet<String> = std::env::vars()
        .filter(|(key, value)| seele_runtime::redact::secret_name(key) && value.len() >= 4)
        .map(|(_, value)| value)
        .collect();
    let output = run(
        Command::new(executable("SEELE_FAILURE_SYSTEMCTL", "systemctl"))
            .args(["--user", "show-environment"]),
        b"",
        3,
        cancel,
    );
    if output.code == 0 {
        for line in output.stdout.lines() {
            if let Some((key, value)) = line.split_once('=') {
                if value.len() >= 4 && seele_runtime::redact::secret_name(key) {
                    identifiers.insert(value.into());
                }
            }
        }
    }
    for name in ["HOME", "USER"] {
        if let Ok(value) = std::env::var(name) {
            identifiers.insert(value);
        }
    }
    let mut hostname = [0u8; 256];
    if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } == 0 {
        let end = hostname
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(hostname.len());
        identifiers.insert(String::from_utf8_lossy(&hostname[..end]).into_owned());
    }
    if let Ok(file) = std::fs::File::open("/etc/machine-id") {
        let mut value = String::new();
        if file.take(128).read_to_string(&mut value).is_ok() {
            identifiers.insert(value.trim().into());
        }
    }
    let mut identifiers: Vec<_> = identifiers.into_iter().filter(|v| v.len() >= 4).collect();
    identifiers.sort_by_key(|value| std::cmp::Reverse(value.len()));
    let mut output = seele_runtime::redact::secrets(report, false);
    for identifier in identifiers {
        output = output.replace(&identifier, "[REDACTED]");
    }
    output
        .chars()
        .filter(|c| seele_runtime::redact::visible(*c))
        .collect()
}
