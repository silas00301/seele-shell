//! Opaque byte-preserving selection tokens; filenames never enter shell code.
use crate::Result;
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE},
    Engine,
};
use serde_json::Value;
use std::ffi::OsString;
use std::io;
use std::os::unix::{ffi::OsStringExt, process::CommandExt};
use std::process::Command;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

fn field(value: &Value) -> Result<Vec<u8>> {
    if let Some(text) = value["text"].as_str() {
        return Ok(text.as_bytes().to_vec());
    }
    Ok(STANDARD.decode(value["bytes"].as_str().ok_or("invalid search field")?)?)
}
pub fn entry(event: &Value) -> Result<Option<String>> {
    if event["type"] != "match" {
        return Ok(None);
    }
    let data = &event["data"];
    let path = field(&data["path"])?;
    let line = data["line_number"]
        .as_u64()
        .filter(|line| *line > 0)
        .ok_or("invalid search line")?;
    let bytes = field(&data["lines"])?;
    let contents = String::from_utf8_lossy(&bytes);
    Ok(Some(format!(
        "{}\t{line}\t{}:{line}: {}",
        URL_SAFE.encode(&path),
        crate::text::path(&path),
        crate::text::terminal(contents.trim_end_matches(['\r', '\n']))
    )))
}
pub fn location(token: &str, line: &str) -> Result<OsString> {
    if token.len() > 32 * 1024
        || token.is_empty()
        || !token
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_=-".contains(&c))
        || line.is_empty()
        || line.starts_with('0')
        || line.len() > 20
        || !line.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("invalid file location".into());
    }
    let path = URL_SAFE.decode(token)?;
    if path.is_empty() || path.contains(&0) {
        return Err("invalid file token".into());
    }
    Ok(OsString::from_vec(path))
}
pub fn run(args: &[String], cancel: &AtomicUsize) -> Result<i32> {
    match args.first().map(String::as_str) {
        Some("source") if args.len() == 1 || (args.len() == 2 && args[1] == "--hidden") => {
            let mut command = Command::new("rg");
            command.args([
                "--json",
                "--line-number",
                "--no-config",
                "--color=never",
                "--glob=!.git",
                "--glob=!.jj",
            ]);
            if args.len() == 2 {
                command.arg("--hidden");
            }
            command.args(["--", ".", "."]);
            let mut frame = Vec::new();
            let mut stdout = seele_runtime::wire::nonblocking_stdout()?;
            let result = seele_runtime::process::stream_stdout(
                &mut command,
                b"",
                seele_runtime::process::Limits {
                    timeout: Duration::from_secs(300),
                    output: 1024 * 1024,
                },
                cancel,
                |bytes| {
                    for chunk in bytes.split_inclusive(|byte| *byte == b'\n') {
                        if frame.len() + chunk.len() > 1024 * 1024 {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                "search record exceeds its limit",
                            ));
                        }
                        frame.extend_from_slice(chunk);
                        if chunk.last() == Some(&b'\n') {
                            let event = serde_json::from_slice(&frame)?;
                            if let Some(row) = entry(&event).map_err(|_| {
                                io::Error::new(io::ErrorKind::InvalidData, "invalid search record")
                            })? {
                                let mut bytes = row.into_bytes();
                                bytes.push(b'\n');
                                seele_runtime::wire::write_bytes(
                                    &mut stdout,
                                    &bytes,
                                    Duration::from_secs(5),
                                    cancel,
                                )?;
                            }
                            frame.clear();
                        }
                    }
                    Ok(())
                },
            );
            match result {
                Ok(result) => Ok(match result.status.code() {
                    Some(0 | 1) => 0,
                    Some(code) => code,
                    None => 1,
                }),
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(0),
                Err(error) => Err(error.into()),
            }
        }
        Some("edit" | "preview") if args.len() == 3 => {
            let path = location(&args[1], &args[2])?;
            let mut command = Command::new(if args[0] == "edit" { "nvim" } else { "bat" });
            if args[0] == "edit" {
                command.arg(format!("+{}", args[2]));
            } else {
                command.args([
                    "--paging=never",
                    "--style=numbers",
                    "--color=always",
                    "--highlight-line",
                    &args[2],
                ]);
            }
            Err(command.arg("--").arg(path).exec().into())
        }
        _ => Err("usage: seele-project-text source [--hidden] | preview|edit TOKEN LINE".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::ffi::OsStrExt;
    #[test]
    fn opaque_paths_roundtrip_without_interpreting_commands_or_controls() {
        for path in [
            b"a:b c'd;$(touch BAD).md".as_slice(),
            b"tab\tnewline\n.txt",
            b"-option.txt",
            b"bad-\xff.txt",
        ] {
            let row = entry(&json!({"type":"match","data":{"path":{"bytes":STANDARD.encode(path)},"line_number":42,"lines":{"text":"hello\tworld\u{1b}[31m\n"}}})).unwrap().unwrap();
            let parts: Vec<_> = row.split('\t').collect();
            assert_eq!(parts.len(), 3);
            assert_eq!(location(parts[0], parts[1]).unwrap().as_bytes(), path);
            assert!(!parts[2].contains(['\n', '\x1b']));
        }
    }
    #[test]
    fn hostile_tokens_and_editor_arguments_fail_closed() {
        for (token, line) in [
            ("';echo BAD", "1"),
            ("ZmlsZQ==", "0"),
            ("ZmlsZQ==", "1;echo BAD"),
            ("AA==", "1"),
        ] {
            assert!(location(token, line).is_err());
        }
    }
}
