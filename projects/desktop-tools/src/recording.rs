//! Explicit, bounded recordings. Nothing uploads automatically or overwrites an original.
use crate::screenshot::{
    command, json_command, local_stamp, private_work, rectangles, resolve, Rectangle,
};
use seele_runtime::process::{capture, discard_detaching, Limits};
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::{
    fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    process::CommandExt,
};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
const MAX: u64 = 512 * 1024 * 1024;
fn checked(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let m = file.metadata()?;
    if !m.is_file()
        || m.uid() != unsafe { libc::geteuid() }
        || m.nlink() != 1
        || m.len() == 0
        || m.len() > MAX
    {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(file)
}
fn duration(file: &File, cancel: &AtomicUsize) -> io::Result<f64> {
    let mut cmd = Command::new("ffprobe");
    let fd = seele_runtime::process::inherit_file(&mut cmd, file)?;
    let result = capture(
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(format!("/proc/self/fd/{fd}")),
        b"",
        Limits {
            timeout: Duration::from_secs(10),
            output: 4096,
        },
        cancel,
    )?;
    let value = std::str::from_utf8(&result.stdout)
        .unwrap_or("")
        .trim()
        .parse::<f64>()
        .unwrap_or(f64::NAN);
    if !result.status.success() || !value.is_finite() || !(0.05..=125.0).contains(&value) {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(value)
}
pub fn trim_range(text: &str, length: f64) -> Option<(f64, f64)> {
    let (start, end) = text.trim().split_once('|')?;
    let (start, end) = (start.parse::<f64>().ok()?, end.parse::<f64>().ok()?);
    (start.is_finite()
        && end.is_finite()
        && start >= 0.0
        && end > start
        && end <= length
        && end - start >= 0.05)
        .then_some((start, end))
}
// A recorder needs a graceful signal to finalize its container. Keep its group
// owned until wait; an unresponsive recorder is killed after five seconds.
struct Recorder(Child);
impl Recorder {
    fn finish(&mut self) -> io::Result<()> {
        if self.0.try_wait()?.is_none() {
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGINT);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.0.try_wait()? {
                return if status.success() {
                    Ok(())
                } else {
                    Err(io::Error::other("recording failed"))
                };
            }
            if Instant::now() >= deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGKILL);
            }
            let _ = self.0.wait();
        }
    }
}
fn dialog(args: &[&str], cancel: &AtomicUsize) -> io::Result<Option<String>> {
    let out = command("zenity", args, b"", 150, 8192, cancel)?;
    if !out.status.success() {
        return Ok(None);
    }
    Ok(Some(
        String::from_utf8(out.stdout)
            .map_err(|_| io::ErrorKind::InvalidData)?
            .trim_end()
            .to_owned(),
    ))
}
fn publish(file: &mut File) -> io::Result<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("HOME").ok_or(io::ErrorKind::NotFound)?)
        .join("Videos/Recordings");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)?;
    seele_runtime::fs::private_directory(&dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(&dir)?;
    tmp.as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    file.seek(SeekFrom::Start(0))?;
    io::copy(&mut file.take(MAX + 1), tmp.as_file_mut())?;
    if tmp.as_file().metadata()?.len() > MAX {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    tmp.as_file().sync_all()?;
    let stamp = local_stamp();
    for index in 0..10000 {
        let path = dir.join(format!("recording-{stamp}-{index}.mp4"));
        match tmp.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.error.kind() == io::ErrorKind::AlreadyExists => tmp = e.file,
            Err(e) => return Err(e.error),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}
fn copy(path: &Path, cancel: &AtomicUsize) -> io::Result<()> {
    let text = path.to_str().ok_or(io::ErrorKind::InvalidData)?;
    // URI-list clipboard allows applications to receive the saved file.
    let encoded = text
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    let status = discard_detaching(
        Command::new("wl-copy").args(["--type", "text/uri-list"]),
        format!("file://{encoded}\r\n").as_bytes(),
        Limits {
            timeout: Duration::from_secs(10),
            output: 4096,
        },
        cancel,
    )?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("clipboard unavailable"))
    }
}
fn pulse(args: &[&str], cancel: &AtomicUsize) -> io::Result<String> {
    let out = command("pactl", args, b"", 5, 1024 * 1024, cancel)?;
    if !out.status.success() {
        return Err(io::Error::other("audio server unavailable"));
    }
    String::from_utf8(out.stdout).map_err(|_| io::ErrorKind::InvalidData.into())
}
fn pulse_list(kind: &str, cancel: &AtomicUsize) -> io::Result<Vec<Value>> {
    let rows: Vec<Value> = serde_json::from_str(&pulse(&["--format=json", "list", kind], cancel)?)?;
    if rows.len() > 4096 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(rows)
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 256
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
fn label(value: &str) -> String {
    value.chars().filter(|c|!c.is_control() && !matches!(*c,'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2060}'..='\u{206f}')).take(120).collect()
}
struct AudioRoute {
    name: String,
    module: u64,
    stream: Value,
    original_sink: u64,
    route_sink: Option<u64>,
}
impl Drop for AudioRoute {
    fn drop(&mut self) {
        let cancel = AtomicUsize::new(0);
        let same = pulse_list("sink-inputs", &cancel).ok().and_then(|rows| {
            rows.into_iter().find(|row| {
                row["index"] == self.stream["index"]
                    && row["client"] == self.stream["client"]
                    && row["sink"].as_u64() == self.route_sink
            })
        });
        if same.is_some() {
            let _ = pulse(
                &[
                    "move-sink-input",
                    &self.stream["index"].to_string(),
                    &self.original_sink.to_string(),
                ],
                &cancel,
            );
        }
        // Revalidate the module through its uniquely named sink before unloading.
        if pulse_list("sinks", &cancel).is_ok_and(|rows| {
            rows.iter().any(|row| {
                row["name"] == self.name
                    && row["owner_module"]
                        .as_u64()
                        .or_else(|| row["owner_module"].as_str()?.parse().ok())
                        == Some(self.module)
            })
        }) {
            let _ = pulse(&["unload-module", &self.module.to_string()], &cancel);
        }
    }
}
fn choose_audio(
    mode: &str,
    work: &Path,
    cancel: &AtomicUsize,
) -> io::Result<Option<(String, Option<AudioRoute>)>> {
    if mode == "Silent" {
        return Ok(Some((String::new(), None)));
    }
    let (kind, heading) = if mode == "Microphone" {
        ("sources", "Select microphone")
    } else {
        ("sink-inputs", "Select one application stream")
    };
    let rows = pulse_list(kind, cancel)?;
    let candidates: Vec<_> = rows
        .into_iter()
        .filter(|row| {
            if mode == "Microphone" {
                row["name"]
                    .as_str()
                    .is_some_and(|name| safe_name(name) && !name.ends_with(".monitor"))
            } else {
                row["index"].as_u64().is_some()
                    && row["client"].as_u64().is_some()
                    && row["sink"].as_u64().is_some()
            }
        })
        .take(64)
        .collect();
    let mut args = vec![
        "--list".to_owned(),
        format!("--title={heading}"),
        "--column=Choice".into(),
        "--column=Audio".into(),
        "--print-column=1".into(),
    ];
    for (index, row) in candidates.iter().enumerate() {
        args.push(index.to_string());
        args.push(label(if mode == "Microphone" {
            row["description"].as_str().unwrap_or("Microphone")
        } else {
            row["properties"]["application.name"]
                .as_str()
                .unwrap_or("Application stream")
        }));
    }
    let refs: Vec<_> = args.iter().map(String::as_str).collect();
    let Some(choice) = dialog(&refs, cancel)? else {
        return Ok(None);
    };
    let row = candidates
        .get(
            choice
                .parse::<usize>()
                .map_err(|_| io::ErrorKind::InvalidInput)?,
        )
        .ok_or(io::ErrorKind::InvalidInput)?;
    if mode == "Microphone" {
        let name = row["name"].as_str().unwrap();
        if !pulse_list(kind, cancel)?
            .iter()
            .any(|item| item["index"] == row["index"] && item["name"] == name)
        {
            return Err(io::ErrorKind::NotFound.into());
        }
        return Ok(Some((name.into(), None)));
    }
    let stream = pulse_list(kind, cancel)?
        .into_iter()
        .find(|item| item["index"] == row["index"] && item["client"] == row["client"])
        .ok_or(io::ErrorKind::NotFound)?;
    let original_sink = stream["sink"].as_u64().unwrap();
    let name = format!(
        "seele_record_{}",
        work.file_name().and_then(|s| s.to_str()).unwrap_or("")
    );
    if !safe_name(&name) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    // Remapping keeps the application audible through its existing output,
    // while the private monitor includes only this explicitly selected stream.
    let module = pulse(
        &[
            "load-module",
            "module-remap-sink",
            &format!("master={original_sink}"),
            &format!("sink_name={name}"),
            "remix=no",
        ],
        cancel,
    )?
    .trim()
    .parse()
    .map_err(|_| io::ErrorKind::InvalidData)?;
    let mut route = AudioRoute {
        name: name.clone(),
        module,
        stream,
        original_sink,
        route_sink: None,
    };
    let sinks = pulse_list("sinks", cancel)?;
    route.route_sink = sinks
        .iter()
        .find(|s| s["name"] == name)
        .and_then(|s| s["index"].as_u64());
    if route.route_sink.is_none() {
        return Err(io::ErrorKind::NotFound.into());
    }
    pulse(
        &["move-sink-input", &route.stream["index"].to_string(), &name],
        cancel,
    )?;
    Ok(Some((format!("{name}.monitor"), Some(route))))
}
pub fn run(cancel: &AtomicUsize) -> io::Result<()> {
    let Some(audio) = dialog(
        &[
            "--list",
            "--radiolist",
            "--title=Record screen",
            "--text=Record up to two minutes. Choose audio explicitly.",
            "--column=Pick",
            "--column=Audio",
            "TRUE",
            "Silent",
            "FALSE",
            "Microphone",
            "FALSE",
            "Application audio",
        ],
        cancel,
    )?
    else {
        return Ok(());
    };
    if !matches!(
        audio.as_str(),
        "Silent" | "Microphone" | "Application audio"
    ) {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let hints = rectangles(
        &json_command("hyprctl", &["monitors", "-j"], cancel)?,
        &json_command("hyprctl", &["clients", "-j"], cancel)?,
    );
    let text = hints
        .iter()
        .map(|r| r.text())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let selected = command("slurp", &[], text.as_bytes(), 120, 4096, cancel)?;
    if !selected.status.success() {
        return Ok(());
    }
    let rect = resolve(
        Rectangle::parse(std::str::from_utf8(&selected.stdout).unwrap_or(""))
            .ok_or(io::ErrorKind::InvalidData)?,
        &hints,
    );
    let work = private_work()?;
    let Some((audio_source, route)) = choose_audio(&audio, work.path(), cancel)? else {
        return Ok(());
    };
    let path = work.path().join("capture.mp4");
    let mut cmd = Command::new("wf-recorder");
    cmd.args([
        "-g",
        &rect.text(),
        "-c",
        "libx264",
        "-x",
        "yuv420p",
        "-r",
        "30",
        "-f",
    ])
    .arg(&path);
    if !audio_source.is_empty() {
        cmd.arg(format!("--audio={audio_source}"));
    }
    let mut recorder = Recorder(
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?,
    );
    // The only mapped dialog after selection is the deliberate recording control.
    let dialog_cancel = std::sync::Arc::new(AtomicUsize::new(0));
    let result = std::thread::scope(|scope| {
        let token = dialog_cancel.clone();
        let control=scope.spawn(move||dialog(&["--question","--title=Recording","--text=Recording the selected area. Stop and review, or discard. Maximum two minutes.","--ok-label=Stop and review","--cancel-label=Discard","--timeout=120"],&token));
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut expired = false;
        while !control.is_finished() {
            if cancel.load(Ordering::Relaxed) != 0
                || Instant::now() >= deadline
                || fs::metadata(&path).is_ok_and(|m| m.len() > MAX)
                || recorder.0.try_wait()?.is_some()
            {
                expired = true;
                dialog_cancel.store(1, Ordering::Relaxed);
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let finished = recorder.finish();
        if finished.is_err() {
            dialog_cancel.store(1, Ordering::Relaxed);
        }
        let outcome = control
            .join()
            .map_err(|_| io::Error::other("recording control failed"))?;
        finished?;
        if expired {
            Ok(None)
        } else {
            outcome
        }
    });
    drop(route);
    if cancel.load(Ordering::Relaxed) != 0 {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let review = result?;
    // Zenity timeout is a safe discard, like Escape.
    if review.is_none() {
        return Ok(());
    }
    let mut source = checked(&path)?;
    let length = duration(&source, cancel)?;
    let saved = publish(&mut source)?;
    copy(&saved, cancel)?;
    let mut final_path = saved.clone();
    let trim_text=format!("Original saved and copied ({length:.2} seconds). Enter start and end in seconds to create a separate trimmed copy. Cancel keeps the original.");
    if let Some(range) = dialog(
        &[
            "--forms",
            "--title=Trim recording",
            &format!("--text={trim_text}"),
            "--add-entry=Start seconds (for example 0)",
            "--add-entry=End seconds",
            "--separator=|",
        ],
        cancel,
    )? {
        let (start, end) = trim_range(&range, length).ok_or(io::ErrorKind::InvalidInput)?;
        let trimmed = work.path().join("trimmed.mp4");
        let mut cmd = Command::new("ffmpeg");
        let original = checked(&saved)?;
        let fd = seele_runtime::process::inherit_file(&mut cmd, &original)?;
        let result = capture(
            cmd.args(["-nostdin", "-v", "error", "-i"])
                .arg(format!("/proc/self/fd/{fd}"))
                .args([
                    "-ss",
                    &start.to_string(),
                    "-t",
                    &(end - start).to_string(),
                    "-map",
                    "0:v:0",
                    "-map",
                    "0:a?",
                    "-c:v",
                    "libx264",
                    "-pix_fmt",
                    "yuv420p",
                    "-c:a",
                    "aac",
                    "-movflags",
                    "+faststart",
                    "-n",
                ])
                .arg(&trimmed),
            b"",
            Limits {
                timeout: Duration::from_secs(180),
                output: 8192,
            },
            cancel,
        )?;
        if !result.status.success() {
            return Err(io::Error::other("trim failed; original retained"));
        }
        let mut file = checked(&trimmed)?;
        duration(&file, cancel)?;
        let path = publish(&mut file)?;
        copy(&path, cancel)?;
        final_path = path;
    }
    share(&final_path, cancel)?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges() {
        assert_eq!(trim_range("0|1.5", 2.0), Some((0.0, 1.5)));
        for text in ["NaN|1", "0|inf", "-1|1", "2|1", "0|3", "0|0.01", "0; rm|1"] {
            assert!(trim_range(text, 2.0).is_none());
        }
    }
}
fn share(path: &Path, cancel: &AtomicUsize) -> io::Result<()> {
    let file = checked(path)?;
    if dialog(&["--question","--title=Share recording?","--text=Upload this recording to 0x0.st, a public third-party host? Anyone with the secret link can view it for 24 hours. Declining keeps the local file copied.","--ok-label=Upload","--cancel-label=Keep local"],cancel)?.is_none(){return Ok(())}
    let mut cmd = Command::new("curl");
    let fd = seele_runtime::process::inherit_file(&mut cmd, &file)?;
    let output = capture(
        cmd.args([
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "10",
            "--max-time",
            "120",
            "--proto",
            "=https",
            "--tlsv1.2",
            "--user-agent",
            "SeeleRecording/1.0",
            "--form",
            &format!("file=@/proc/self/fd/{fd};filename=recording.mp4;type=video/mp4"),
            "--form-string",
            "secret=",
            "--form-string",
            "expires=24",
            "https://0x0.st",
        ]),
        b"",
        Limits {
            timeout: Duration::from_secs(125),
            output: 4096,
        },
        cancel,
    )?;
    let link = std::str::from_utf8(&output.stdout).unwrap_or("").trim();
    if !output.status.success()
        || !link.strip_prefix("https://0x0.st/").is_some_and(|s| {
            !s.is_empty()
                && s.len() <= 200
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._~-".contains(&b))
        })
    {
        return Err(io::Error::other("upload failed; local file retained"));
    }
    let status = discard_detaching(
        Command::new("wl-copy").args(["--type", "text/plain"]),
        link.as_bytes(),
        Limits {
            timeout: Duration::from_secs(10),
            output: 4096,
        },
        cancel,
    )?;
    if !status.success() {
        return Err(io::Error::other("link clipboard unavailable"));
    }
    Ok(())
}
