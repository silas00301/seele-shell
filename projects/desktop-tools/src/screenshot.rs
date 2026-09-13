//! Frozen selection retains the existing picker/dialog presentation. Private
//! working files are never the public output pathname or an upload argument.
use seele_runtime::process::{capture, discard_detaching, discard_file_detaching, Limits};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
const MAX_IMAGE: u64 = 128 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Rectangle {
    pub x: i64,
    pub y: i64,
    pub width: u64,
    pub height: u64,
}
impl Rectangle {
    fn valid(self) -> bool {
        self.x.unsigned_abs() <= 1_000_000
            && self.y.unsigned_abs() <= 1_000_000
            && self.width > 0
            && self.height > 0
            && self.width <= 65535
            && self.height <= 65535
            && self.width * self.height <= 268_435_456
    }
    fn area(self) -> u64 {
        self.width * self.height
    }
    fn contains(self, x: i64, y: i64) -> bool {
        x >= self.x
            && x < self.x + self.width as i64
            && y >= self.y
            && y < self.y + self.height as i64
    }
    fn text(self) -> String {
        format!("{},{} {}x{}", self.x, self.y, self.width, self.height)
    }
    pub fn parse(value: &str) -> Option<Self> {
        let mut values = value.split_whitespace();
        let (x, y) = values.next()?.split_once(',')?;
        let (width, height) = values.next()?.split_once('x')?;
        if values.next().is_some() {
            return None;
        }
        let result = Self {
            x: x.parse().ok()?,
            y: y.parse().ok()?,
            width: width.parse().ok()?,
            height: height.parse().ok()?,
        };
        result.valid().then_some(result)
    }
}
pub fn rectangles(monitors: &Value, clients: &Value) -> Vec<Rectangle> {
    let mut rectangles = vec![];
    let mut visible = BTreeSet::new();
    if let Some(monitors) = monitors.as_array() {
        for monitor in monitors.iter().take(64) {
            let scale = monitor["scale"].as_f64().unwrap_or(1.0);
            if !scale.is_finite() || scale <= 0.0 {
                continue;
            }
            let (mut width, mut height) = (
                (monitor["width"].as_f64().unwrap_or(0.0) / scale).floor() as u64,
                (monitor["height"].as_f64().unwrap_or(0.0) / scale).floor() as u64,
            );
            if matches!(monitor["transform"].as_u64(), Some(1 | 3 | 5 | 7)) {
                std::mem::swap(&mut width, &mut height);
            }
            if let (Some(x), Some(y)) = (monitor["x"].as_i64(), monitor["y"].as_i64()) {
                let rect = Rectangle {
                    x,
                    y,
                    width,
                    height,
                };
                if rect.valid() {
                    rectangles.push(rect);
                }
            }
            if let Some(id) = monitor["activeWorkspace"]["id"].as_i64() {
                visible.insert(id);
            }
            if let Some(id) = monitor["specialWorkspace"]["id"]
                .as_i64()
                .filter(|id| *id != 0)
            {
                visible.insert(id);
            }
        }
    }
    let mut windows = BTreeSet::new();
    if let Some(clients) = clients.as_array() {
        for client in clients.iter().take(16384) {
            if client["mapped"] == false
                || client["hidden"] == true
                || (client["pinned"] != true
                    && !client["workspace"]["id"]
                        .as_i64()
                        .is_some_and(|id| visible.contains(&id)))
            {
                continue;
            }
            if let (Some(x), Some(y), Some(width), Some(height)) = (
                client["at"][0].as_i64(),
                client["at"][1].as_i64(),
                client["size"][0].as_u64(),
                client["size"][1].as_u64(),
            ) {
                let rect = Rectangle {
                    x,
                    y,
                    width,
                    height,
                };
                if rect.valid() {
                    windows.insert(rect.text());
                }
            }
        }
    }
    rectangles.extend(windows.into_iter().filter_map(|v| Rectangle::parse(&v)));
    rectangles
}
pub fn resolve(selection: Rectangle, hints: &[Rectangle]) -> Rectangle {
    if selection.area() < 20 {
        hints
            .iter()
            .copied()
            .filter(|rect| rect.contains(selection.x, selection.y))
            .min_by_key(|rect| rect.area())
            .unwrap_or(selection)
    } else {
        selection
    }
}
fn command(
    name: &str,
    args: &[&str],
    input: &[u8],
    timeout: u64,
    limit: usize,
    cancel: &AtomicUsize,
) -> io::Result<seele_runtime::process::Output> {
    capture(
        Command::new(name).args(args),
        input,
        Limits {
            timeout: Duration::from_secs(timeout),
            output: limit,
        },
        cancel,
    )
}
fn json_command(name: &str, args: &[&str], cancel: &AtomicUsize) -> io::Result<Value> {
    let output = command(name, args, b"", 5, 4 * 1024 * 1024, cancel)?;
    if !output.status.success() {
        return Err(io::Error::other("desktop state unavailable"));
    }
    serde_json::from_slice(&output.stdout).map_err(Into::into)
}
struct Freeze {
    cancel: Arc<AtomicUsize>,
    thread: Option<std::thread::JoinHandle<io::Result<()>>>,
}
impl Freeze {
    fn start() -> Self {
        let cancel = Arc::new(AtomicUsize::new(0));
        let token = cancel.clone();
        let thread = std::thread::spawn(move || {
            command("hyprpicker", &["-r", "-z"], b"", 900, 4096, &token).map(|_| ())
        });
        Self {
            cancel,
            thread: Some(thread),
        }
    }
    fn running(&self) -> bool {
        self.thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
    }
    fn stop(&mut self) {
        self.cancel.store(1, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for Freeze {
    fn drop(&mut self) {
        self.stop();
    }
}
fn private_work() -> io::Result<tempfile::TempDir> {
    let runtime = if let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") {
        let path = PathBuf::from(path);
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        path
    } else {
        std::env::temp_dir()
    };
    tempfile::Builder::new()
        .prefix("seele-screenshot-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(runtime)
}
fn image(path: &Path) -> io::Result<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.len() > MAX_IMAGE
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic)?;
    if magic != *b"\x89PNG\r\n\x1a\n" {
        return Err(io::ErrorKind::InvalidData.into());
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(file)
}
fn local_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as libc::time_t;
    unsafe {
        let mut local: libc::tm = std::mem::zeroed();
        let mut output = [0 as libc::c_char; 32];
        if libc::localtime_r(&seconds, &mut local).is_null()
            || libc::strftime(
                output.as_mut_ptr(),
                output.len(),
                c"%Y-%m-%d_%H-%M-%S".as_ptr(),
                &local,
            ) == 0
        {
            return seconds.to_string();
        }
        std::ffi::CStr::from_ptr(output.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}
pub fn publish(source: &mut File, directory: &Path, stamp: &str) -> io::Result<PathBuf> {
    let mut temporary = tempfile::Builder::new()
        .prefix(".seele-screenshot-")
        .tempfile_in(directory)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    source.seek(SeekFrom::Start(0))?;
    io::copy(&mut source.take(MAX_IMAGE + 1), temporary.as_file_mut())?;
    if temporary.as_file().metadata()?.len() > MAX_IMAGE {
        return Err(io::ErrorKind::FileTooLarge.into());
    }
    temporary.as_file().sync_all()?;
    for suffix in 0..10000 {
        let name = if suffix == 0 {
            format!("screenshot-{stamp}.png")
        } else {
            format!("screenshot-{stamp}-{suffix}.png")
        };
        let path = directory.join(name);
        match temporary.persist_noclobber(&path) {
            Ok(_) => return Ok(path),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                temporary = error.file
            }
            Err(error) => return Err(error.error),
        }
    }
    Err(io::ErrorKind::AlreadyExists.into())
}
fn copy_image(file: &File, cancel: &AtomicUsize) -> io::Result<()> {
    let mut file = file;
    file.seek(SeekFrom::Start(0))?;
    let status = discard_file_detaching(
        Command::new("wl-copy").args(["--type", "image/png"]),
        file,
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
fn notify(title: &str, body: &str, critical: bool, cancel: &AtomicUsize) {
    let mut args = vec!["--transient"];
    if critical {
        args.push("--urgency=critical");
    }
    args.extend(["--", title, body]);
    let _ = command("notify-send", &args, b"", 5, 4096, cancel);
}
fn upload(file: &File, cancel: &AtomicUsize) -> io::Result<()> {
    let mut source = file;
    source.seek(SeekFrom::Start(0))?;
    let mut command = Command::new("curl");
    let descriptor = seele_runtime::process::inherit_file(&mut command, file)?;
    let form = format!("file=@/proc/self/fd/{descriptor};filename=screenshot.png;type=image/png");
    // -q must be first: local curl config must not change destination, leak
    // credentials, follow redirects, or silently bypass TLS verification.
    command.args([
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
        "SeeleScreenshot/1.0 (+https://github.com/silas00301/seele)",
        "--form",
        &form,
        "--form-string",
        "secret=",
        "--form-string",
        "expires=24",
        "https://0x0.st",
    ]);
    let result = capture(
        &mut command,
        b"",
        Limits {
            timeout: Duration::from_secs(125),
            output: 4096,
        },
        cancel,
    );
    if let Ok(output) = result {
        let response = std::str::from_utf8(&output.stdout).unwrap_or("").trim();
        if output.status.success()
            && response
                .strip_prefix("https://0x0.st/")
                .is_some_and(|suffix| {
                    !suffix.is_empty()
                        && suffix.len() <= 200
                        && suffix
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._~-".contains(&b))
                })
        {
            let copied = command_output_copy(response, cancel);
            if copied.is_ok() {
                notify(
                    "Screenshot link copied",
                    "The public 0x0.st link expires in 24 hours.",
                    false,
                    cancel,
                );
                return Ok(());
            }
        }
    }
    copy_image(file, cancel)?;
    notify(
        "Screenshot upload failed",
        "The image stayed local and was copied instead.",
        true,
        cancel,
    );
    Ok(())
}
fn command_output_copy(text: &str, cancel: &AtomicUsize) -> io::Result<()> {
    let status = discard_detaching(
        Command::new("wl-copy").args(["--type", "text/plain"]),
        text.as_bytes(),
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
pub fn run(mode: &str, cancel: &AtomicUsize) -> io::Result<()> {
    if !matches!(mode, "capture" | "annotate" | "upload") {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let home = std::env::var_os("HOME").ok_or(io::ErrorKind::NotFound)?;
    let output = PathBuf::from(home).join("Pictures/Screenshots");
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&output)?;
    let output = fs::canonicalize(output)?;
    seele_runtime::fs::private_directory(&output)?;
    let work = private_work()?;
    let (monitors, clients) = std::thread::scope(|scope| {
        let monitors = scope.spawn(|| json_command("hyprctl", &["monitors", "-j"], cancel));
        let clients = json_command("hyprctl", &["clients", "-j"], cancel);
        Ok::<_, io::Error>((
            monitors
                .join()
                .map_err(|_| io::Error::other("desktop query failed"))?,
            clients,
        ))
    })?;
    let hints = rectangles(&monitors?, &clients?);
    let hints_text = hints
        .iter()
        .map(|r| r.text())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut freeze = Freeze::start();
    std::thread::sleep(Duration::from_millis(100));
    if !freeze.running() {
        return Err(io::Error::other("desktop freeze unavailable"));
    }
    let selection = command("slurp", &[], hints_text.as_bytes(), 900, 4096, cancel)?;
    if !selection.status.success() {
        return Ok(());
    }
    let selected = Rectangle::parse(std::str::from_utf8(&selection.stdout).unwrap_or(""))
        .ok_or(io::ErrorKind::InvalidData)?;
    let selected = resolve(selected, &hints);
    if !freeze.running() {
        return Err(io::Error::other("desktop freeze ended"));
    }
    let captured = work.path().join("capture.png");
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&captured)?;
    let result = capture(
        Command::new("grim")
            .arg("-g")
            .arg(selected.text())
            .arg(&captured),
        b"",
        Limits {
            timeout: Duration::from_secs(30),
            output: 4096,
        },
        cancel,
    )?;
    if !result.status.success() {
        return Err(io::Error::other("capture failed"));
    }
    if !freeze.running() {
        return Err(io::Error::other("desktop freeze ended"));
    }
    freeze.stop();
    let path = if mode == "annotate" {
        let annotated = work.path().join("annotated.png");
        let result = capture(
            Command::new("satty")
                .arg("--filename")
                .arg(&captured)
                .arg("--output-filename")
                .arg(&annotated)
                .args([
                    "--initial-tool",
                    "arrow",
                    "--early-exit",
                    "--actions-on-enter",
                    "save-to-file",
                    "--actions-on-escape",
                    "exit",
                ]),
            b"",
            Limits {
                timeout: Duration::from_secs(3600),
                output: 64 * 1024,
            },
            cancel,
        )?;
        if !result.status.success() {
            return Err(io::Error::other("annotation failed"));
        }
        if fs::metadata(&annotated).map_or(true, |v| v.len() == 0) {
            return Ok(());
        }
        annotated
    } else {
        captured
    };
    let mut image = image(&path)?;
    publish(&mut image, &output, &local_stamp())?;
    image.seek(SeekFrom::Start(0))?;
    if mode == "upload" {
        let response=command("zenity",&["--question","--title=Upload screenshot?","--icon-name=dialog-warning","--width=460","--ok-label=Upload","--cancel-label=Copy image","--text=This sends the screenshot to 0x0.st, a public third-party host.\n\nAnyone with the secret link can view it for 24 hours. Only upload images that contain no private or sensitive information."],b"",900,4096,cancel);
        if response.is_ok_and(|v| v.status.success()) {
            return upload(&image, cancel);
        }
    }
    copy_image(&image, cancel)
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn scale_rotation_visibility_and_click_resolution() {
        let monitors = json!([{"x":-1080,"y":0,"width":1920,"height":1080,"scale":1,"transform":1,"activeWorkspace":{"id":1},"specialWorkspace":{"id":-99}}]);
        let clients = json!([{"at":[-1000,20],"size":[300,200],"workspace":{"id":1}},{"at":[-1000,20],"size":[200,100],"workspace":{"id":2}},{"at":[-1000,20],"size":[100,100],"workspace":{"id":-99}}]);
        let hints = rectangles(&monitors, &clients);
        assert_eq!(hints.len(), 3);
        assert_eq!(hints[0].text(), "-1080,0 1080x1920");
        assert_eq!(
            resolve(Rectangle::parse("-999,21 1x1").unwrap(), &hints).text(),
            "-1000,20 100x100"
        );
        assert!(Rectangle::parse("0,0 999999999999x1").is_none());
    }
    #[test]
    fn exclusive_publication_never_overwrites_existing_names() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let mut source = tempfile::tempfile().unwrap();
        source.write_all(b"image").unwrap();
        let first = publish(&mut source, root.path(), "fixture").unwrap();
        let second = publish(&mut source, root.path(), "fixture").unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), b"image");
        assert_eq!(fs::metadata(second).unwrap().mode() & 0o777, 0o600);
    }
}
