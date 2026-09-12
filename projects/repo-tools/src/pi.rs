//! Bounded Jujutsu metadata for Pi lifecycle callbacks. Never emits diagnostics
//! or command output on failure; displayed text is sanitized by native UI policy.
use crate::Result;
use seele_runtime::process::{capture, Limits};
use serde_json::json;
use std::{path::Path, process::Command, sync::atomic::AtomicUsize, time::Duration};
const TEMPLATE: &str =
    "if(self.local_bookmarks(), self.local_bookmarks().join(\",\"), change_id.shortest(8))";
fn run(binary: &Path, arguments: &[&str], cancel: &AtomicUsize) -> std::io::Result<Option<String>> {
    let output = capture(
        Command::new(binary).args(arguments),
        b"",
        Limits {
            timeout: Duration::from_secs(2),
            output: 64 * 1024,
        },
        cancel,
    )?;
    if !output.status.success() {
        return Ok(None);
    }
    String::from_utf8(output.stdout)
        .map(Some)
        .map_err(|_| std::io::ErrorKind::InvalidData.into())
}
pub fn revision(arguments: &[String], cancel: &AtomicUsize) -> Result<()> {
    let [binary, mode] = arguments else {
        return Err(2);
    };
    if !Path::new(binary).is_absolute() || !matches!(mode.as_str(), "detect" | "revision") {
        return Err(2);
    }
    let repository = mode == "revision"
        || run(Path::new(binary), &["root"], cancel)
            .map_err(|_| 1)?
            .is_some();
    let revision = if repository {
        run(
            Path::new(binary),
            &[
                "log",
                "--no-graph",
                "--no-pager",
                "--color=never",
                "-r",
                "@",
                "-T",
                TEMPLATE,
            ],
            cancel,
        )
        .map_err(|_| 1)?
        .unwrap_or("?".into())
    } else {
        String::new()
    };
    serde_json::to_writer(
        std::io::stdout().lock(),
        &json!({"repository":repository,"revision":revision}),
    )
    .map_err(|_| 1)
}
