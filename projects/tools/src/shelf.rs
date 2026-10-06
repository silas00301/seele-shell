//! One shell-owned temporary shelf. Original files are references; explicit
//! text is private runtime data, removed with its item or worker lifetime.
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::{
    fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    net::{UnixListener, UnixStream},
};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicUsize, Arc};
use std::time::Duration;
use url::Url;
const MAX_ITEMS: usize = 64;
const MAX_TEXT: usize = 64 * 1024;
const MAX_FRAME: usize = 256 * 1024;
struct Item {
    id: u64,
    path: PathBuf,
    text: bool,
    selected: bool,
    caption: String,
}
struct Shelf {
    root: tempfile::TempDir,
    items: Vec<Item>,
    serial: u64,
    cancel: Arc<AtomicUsize>,
}
impl Shelf {
    fn new(runtime: &Path, cancel: Arc<AtomicUsize>) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(runtime)?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let root = tempfile::Builder::new()
            .prefix("seele-shelf-")
            .tempdir_in(runtime)?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700))?;
        Ok(Self {
            root,
            items: Vec::new(),
            serial: 0,
            cancel,
        })
    }
    fn snapshot(&self, error: &str) -> Value {
        json!({"version":1,"error":error,"items":self.items.iter().map(|item| {
            let metadata=fs::metadata(&item.path).ok().filter(|m| m.is_file());
            let name=if item.text { format!("Text snippet {}",item.id) } else { item.path.file_name().map(|n|n.to_string_lossy().into_owned()).unwrap_or_default() };
            let extension=item.path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
            json!({"id":item.id.to_string(),"name":label(&name),"caption":item.caption,"path":item.path,"uri":Url::from_file_path(&item.path).ok().map(|u| u.to_string()).unwrap_or_default(),"kind":if item.text {"text"} else {"file"},"image":matches!(extension.as_str(),"png"|"jpg"|"jpeg"|"webp"|"gif"),"selected":item.selected,"available":metadata.is_some(),"bytes":metadata.map(|m| m.len()).unwrap_or(0)})
        }).collect::<Vec<_>>()})
    }
    fn apply(&mut self, value: &Value) -> Result<(), &'static str> {
        match value["op"].as_str().unwrap_or("") {
            "status" => (),
            "files" => {
                let paths = value["paths"]
                    .as_array()
                    .filter(|paths| !paths.is_empty() && paths.len() <= MAX_ITEMS)
                    .ok_or("Choose one to 64 files")?;
                let mut incoming = Vec::new();
                for value in paths {
                    let text = value
                        .as_str()
                        .filter(|s| s.len() <= 4096)
                        .ok_or("Invalid file reference")?;
                    let path = if text.starts_with("file:") {
                        let uri = Url::parse(text).map_err(|_| "Use local files only")?;
                        if uri.scheme() != "file"
                            || uri
                                .host_str()
                                .is_some_and(|h| !h.is_empty() && h != "localhost")
                        {
                            return Err("Use local files only");
                        }
                        uri.to_file_path().map_err(|_| "Invalid local file")?
                    } else {
                        PathBuf::from(text)
                    };
                    if !path.is_absolute() {
                        return Err("Use absolute local file paths");
                    }
                    let path = path
                        .canonicalize()
                        .map_err(|_| "A collected file is unavailable")?;
                    if path.to_str().is_none() {
                        return Err("Use UTF-8 local file paths");
                    }
                    if !fs::metadata(&path).is_ok_and(|m| m.is_file()) {
                        return Err("The shelf collects regular files only");
                    }
                    if !incoming.contains(&path) {
                        incoming.push(path);
                    }
                }
                let additional = incoming
                    .iter()
                    .filter(|path| !self.items.iter().any(|item| item.path == **path))
                    .count();
                if self.items.len() + additional > MAX_ITEMS {
                    return Err("The shelf holds up to 64 items");
                }
                for path in incoming {
                    if let Some(item) = self.items.iter_mut().find(|item| item.path == path) {
                        item.selected = true;
                        continue;
                    }
                    self.serial += 1;
                    self.items.push(Item {
                        id: self.serial,
                        path,
                        text: false,
                        selected: true,
                        caption: String::new(),
                    });
                }
            }
            "text" => self.text(value["text"].as_str().ok_or("Text is required")?)?,
            "clipboard" => {
                let output = seele_runtime::process::capture(
                    std::process::Command::new("wl-paste").args(["--type", "text", "--no-newline"]),
                    b"",
                    seele_runtime::process::Limits {
                        timeout: Duration::from_secs(3),
                        output: MAX_TEXT,
                    },
                    &self.cancel,
                )
                .map_err(|_| "Clipboard text is unavailable or too large")?;
                if !output.status.success() {
                    return Err("Clipboard text is unavailable");
                }
                self.text(
                    std::str::from_utf8(&output.stdout).map_err(|_| "Clipboard is not text")?,
                )?;
            }
            "select" => {
                let id = value["id"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .ok_or("Invalid shelf item")?;
                let item = self
                    .items
                    .iter_mut()
                    .find(|item| item.id == id)
                    .ok_or("Shelf item no longer exists")?;
                item.selected = !item.selected;
            }
            "all" => {
                for item in &mut self.items {
                    item.selected = value["selected"].as_bool().ok_or("Invalid selection")?;
                }
            }
            "remove" | "clear" => {
                let clear = value["op"] == "clear";
                let mut retained = Vec::new();
                for item in self.items.drain(..) {
                    if clear || item.selected {
                        if item.text {
                            let _ = fs::remove_file(&item.path);
                        }
                    } else {
                        retained.push(item);
                    }
                }
                self.items = retained;
            }
            _ => return Err("Unknown shelf action"),
        }
        Ok(())
    }
    fn text(&mut self, text: &str) -> Result<(), &'static str> {
        if text.is_empty() || text.len() > MAX_TEXT {
            return Err("Text must fit in 64 KiB");
        }
        if self.items.len() >= MAX_ITEMS || self.items.iter().filter(|i| i.text).count() >= 16 {
            return Err("The shelf holds up to 16 text snippets and 64 items");
        }
        self.serial += 1;
        let path = self
            .root
            .path()
            .join(format!("Text snippet {}.txt", self.serial));
        seele_runtime::fs::atomic_write_new(&path, text.as_bytes())
            .map_err(|_| "Could not collect text")?;
        self.items.push(Item {
            id: self.serial,
            path,
            text: true,
            selected: true,
            caption: label(text.lines().next().unwrap_or("")),
        });
        Ok(())
    }
}
fn label(text: &str) -> String {
    text.chars().filter(|c| !c.is_control() && !matches!(*c,'\u{200b}'..='\u{200f}'|'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'|'\u{feff}')).take(120).collect()
}
pub fn run() -> io::Result<()> {
    use std::sync::atomic::{AtomicBool, Ordering};
    let runtime =
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or(io::ErrorKind::NotFound)?);
    let stop = seele_runtime::process::termination_signal()?;
    let mut shelf = Shelf::new(&runtime, stop.clone())?;
    let socket_path = runtime.join("seele-shelf.sock");
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(runtime.join("seele-shelf.lock"))?;
    let info = lock.metadata()?;
    if !info.is_file()
        || info.uid() != unsafe { libc::geteuid() }
        || info.mode() & 0o077 != 0
        || unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Shelf already running or unsafe lock",
        ));
    }
    match fs::symlink_metadata(&socket_path) {
        Ok(info) => {
            if !info.file_type().is_socket() || info.uid() != unsafe { libc::geteuid() } {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Unsafe shelf socket",
                ));
            }
            match UnixStream::connect(&socket_path) {
                Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
                    fs::remove_file(&socket_path)?
                }
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Shelf socket is active",
                    ))
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let listener = UnixListener::bind(&socket_path)?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))?;
    listener.set_nonblocking(true)?;
    struct SocketGuard {
        path: PathBuf,
        inode: u64,
        device: u64,
    }
    impl Drop for SocketGuard {
        fn drop(&mut self) {
            if fs::symlink_metadata(&self.path)
                .is_ok_and(|m| m.ino() == self.inode && m.dev() == self.device)
            {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
    let metadata = fs::symlink_metadata(&socket_path)?;
    let _socket = SocketGuard {
        path: socket_path,
        inode: metadata.ino(),
        device: metadata.dev(),
    };
    let mut output = seele_runtime::wire::nonblocking_stdout()?;
    let emit =
        |output: &mut seele_runtime::wire::NonblockingFile, value: Value| -> io::Result<()> {
            let bytes = seele_runtime::wire::json_frame(&value, MAX_FRAME * 8)?;
            seele_runtime::wire::write_bytes(
                output,
                &bytes,
                Duration::from_secs(2),
                &AtomicBool::new(false),
            )
        };
    emit(&mut output, shelf.snapshot(""))?;
    // Poll between bounded reads so an idle worker still removes text on SIGTERM.
    let flags = unsafe { libc::fcntl(0, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(0, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut input = io::stdin().lock();
    let mut pending = Vec::new();
    let mut chunk = [0u8; 8192];
    while stop.load(Ordering::Relaxed) == 0 {
        let mut poll = [
            libc::pollfd {
                fd: 0,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let ready = unsafe { libc::poll(poll.as_mut_ptr(), 2, 100) };
        if ready < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            continue;
        }
        if poll[1].revents & libc::POLLIN != 0 {
            if let Ok((mut client, _)) = listener.accept() {
                client.set_read_timeout(Some(Duration::from_secs(2)))?;
                client.set_write_timeout(Some(Duration::from_secs(2)))?;
                if seele_runtime::wire::same_uid(&client)? {
                    let mut frame = Vec::new();
                    let read = seele_runtime::wire::read_frame(
                        &mut BufReader::new(&client),
                        &mut frame,
                        MAX_FRAME,
                    );
                    let error = match read {
                        Ok(true) if frame.ends_with(b"\n") => {
                            match serde_json::from_slice::<Value>(&frame) {
                                Ok(value) => shelf.apply(&value).err().unwrap_or(""),
                                Err(_) => "Invalid shelf request",
                            }
                        }
                        _ => "Invalid shelf request",
                    };
                    emit(&mut output, shelf.snapshot(error))?;
                    let reply = json!({"ok":error.is_empty(),"error":error});
                    let _ = writeln!(client, "{reply}");
                }
            }
        }
        if poll[0].revents == 0 {
            continue;
        }
        let count = match input.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                continue
            }
            Err(e) => return Err(e),
        };
        for byte in &chunk[..count] {
            pending.push(*byte);
            if pending.len() > MAX_FRAME {
                return Err(io::ErrorKind::InvalidData.into());
            }
            if *byte == b'\n' {
                let error = match serde_json::from_slice::<Value>(&pending) {
                    Ok(value) => shelf.apply(&value).err().unwrap_or(""),
                    Err(_) => "Invalid shelf request",
                };
                emit(&mut output, shelf.snapshot(error))?;
                pending.clear();
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_batches_are_atomic_deduplicated_and_never_deleted() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.txt");
        fs::write(&source, "original").unwrap();
        let mut shelf = Shelf::new(root.path(), Arc::new(AtomicUsize::new(0))).unwrap();
        shelf
            .apply(&json!({"op":"files","paths":[source,source]}))
            .unwrap();
        assert_eq!(shelf.items.len(), 1);
        assert!(shelf
            .apply(&json!({"op":"files","paths":[source,root.path().join("missing")]}))
            .is_err());
        assert_eq!(shelf.items.len(), 1);
        shelf.apply(&json!({"op":"clear"})).unwrap();
        assert_eq!(fs::read_to_string(source).unwrap(), "original");
    }
    #[test]
    fn text_and_cleanup_are_private_bounded_and_temporary() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path;
        {
            let mut shelf = Shelf::new(root.path(), Arc::new(AtomicUsize::new(0))).unwrap();
            shelf.text("hello\nworld").unwrap();
            path = shelf.items[0].path.clone();
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert!(shelf.text(&"a".repeat(MAX_TEXT + 1)).is_err());
            assert_eq!(shelf.snapshot("")["items"][0]["caption"], "hello");
        }
        assert!(!path.exists());
        assert_eq!(label("\u{202e}hello\n"), "hello");
    }
    #[test]
    fn remote_uris_and_directories_are_refused() {
        let root = tempfile::tempdir().unwrap();
        let mut shelf = Shelf::new(root.path(), Arc::new(AtomicUsize::new(0))).unwrap();
        for path in [
            "file://remote.example/file".to_owned(),
            root.path().to_string_lossy().into_owned(),
            "relative".into(),
        ] {
            assert!(shelf.apply(&json!({"op":"files","paths":[path]})).is_err());
        }
        assert!(shelf.items.is_empty());
    }
}
