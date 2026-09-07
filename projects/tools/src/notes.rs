//! Local notes and recoverable voice memos. Text never travels in argv.
use crate::command::{epoch, home};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SAMPLE_RATE: u32 = 16_000;
const MAX_AUDIO: u32 = SAMPLE_RATE * 2 * 60 * 60;
static STOP: AtomicBool = AtomicBool::new(false);

#[derive(Deserialize, Serialize)]
struct Note {
    id: String,
    title: String,
    body: String,
    created: i64,
    updated: i64,
    #[serde(default)]
    trashed: bool,
}

fn identifier() -> String {
    format!(
        "{:032x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    )
}

fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn private_dir(path: &Path) -> Result {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(())
}

fn private_file(path: &Path, create_new: bool) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(!create_new)
        .create_new(create_new)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

fn lock(path: &Path) -> Result<File> {
    let file = private_file(path, false)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("This note is recording, or another Notes session is writing".into());
    }
    Ok(file)
}

struct Library(PathBuf);
impl Library {
    fn directory(&self, id: &str) -> Result<PathBuf> {
        if !valid_id(id) {
            return Err("Invalid note identifier".into());
        }
        let path = self.0.join(id);
        if !fs::symlink_metadata(&path)?.is_dir() {
            return Err("Invalid note directory".into());
        }
        Ok(path)
    }

    fn read(&self, id: &str) -> Result<Note> {
        let path = self.directory(id)?.join("note.json");
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?;
        let note: Note = serde_json::from_reader(file)?;
        if note.id != id {
            return Err("Note identity does not match its directory".into());
        }
        Ok(note)
    }

    fn write(&self, note: &Note) -> Result {
        let dir = self.directory(&note.id)?;
        let temporary = dir.join(format!("{}.tmp", identifier()));
        let result = (|| -> Result {
            let mut file = private_file(&temporary, true)?;
            serde_json::to_writer(&mut file, note)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            fs::rename(&temporary, dir.join("note.json"))?;
            File::open(&dir)?.sync_all()?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }

    fn view(&self, note: &Note) -> Result<Value> {
        let dir = self.directory(&note.id)?;
        let mut memos = Vec::new();
        let mut updated = note.updated;
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let id = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
            if !valid_id(id)
                || !entry.file_type()?.is_file()
                || path.extension().and_then(|v| v.to_str()) != Some("wav")
            {
                continue;
            }
            let metadata = entry.metadata()?;
            let created = metadata.modified()?.duration_since(UNIX_EPOCH)?.as_secs() as i64;
            updated = updated.max(created);
            let bytes = metadata.len().saturating_sub(44);
            memos.push(
                json!({"id":id,"path":path,"created":created,"duration":bytes * 1000 / (SAMPLE_RATE as u64 * 2)}),
            );
        }
        memos.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        let mut value = serde_json::to_value(note)?;
        value["memos"] = json!(memos);
        value["updated"] = json!(updated);
        Ok(value)
    }

    fn list(&self) -> Result<Value> {
        let mut notes = Vec::new();
        let mut unreadable = 0;
        for entry in fs::read_dir(&self.0)? {
            let entry = entry?;
            let id = entry.file_name().to_string_lossy().into_owned();
            if !valid_id(&id) || !entry.file_type()?.is_dir() {
                continue;
            }
            match self.read(&id).and_then(|note| self.view(&note)) {
                Ok(note) => notes.push(note),
                Err(_) => unreadable += 1,
            }
        }
        notes.sort_by_key(|note| std::cmp::Reverse(note["updated"].as_i64().unwrap_or(0)));
        Ok(json!({"notes":notes,"unreadable":unreadable}))
    }

    fn request(&self, message: &Value) -> Result<Value> {
        let action = message["action"].as_str().unwrap_or("");
        if action == "list" {
            return self.list();
        }
        if action == "create" {
            let id = identifier();
            fs::DirBuilder::new().mode(0o700).create(self.0.join(&id))?;
            let note = Note {
                id,
                title: String::new(),
                body: String::new(),
                created: epoch(),
                updated: epoch(),
                trashed: false,
            };
            self.write(&note)?;
            return Ok(json!({"note":self.view(&note)?}));
        }
        let id = message["id"].as_str().ok_or("Note identifier required")?;
        let mut note = self.read(id)?;
        match action {
            "save" => {
                if note.trashed {
                    return Err("Restore this note before editing it".into());
                }
                let title = message["title"].as_str().ok_or("Title required")?;
                let body = message["body"].as_str().ok_or("Body required")?;
                if title.len() > 4096 || body.len() > 2 * 1024 * 1024 {
                    return Err("Note exceeds the 2 MiB text limit".into());
                }
                note.title = title.into();
                note.body = body.into();
            }
            "trash" | "restore" => {
                let _recording = lock(&self.directory(id)?.join(".recording"))?;
                note.trashed = action == "trash";
                note.updated = epoch();
                self.write(&note)?;
                return Ok(json!({"note":self.view(&note)?}));
            }
            _ => return Err("Unknown Notes action".into()),
        }
        note.updated = epoch();
        self.write(&note)?;
        Ok(json!({"note":self.view(&note)?}))
    }
}

fn emit(value: &Value) -> Result {
    let mut out = io::stdout().lock();
    writeln!(out, "{value}")?;
    out.flush()?;
    Ok(())
}

fn wav_header(bytes: u32) -> [u8; 44] {
    let mut header = [0; 44];
    header[0..4].copy_from_slice(b"RIFF");
    header[4..8].copy_from_slice(&(bytes + 36).to_le_bytes());
    header[8..16].copy_from_slice(b"WAVEfmt ");
    header[16..20].copy_from_slice(&16_u32.to_le_bytes());
    header[20..22].copy_from_slice(&1_u16.to_le_bytes());
    header[22..24].copy_from_slice(&1_u16.to_le_bytes());
    header[24..28].copy_from_slice(&SAMPLE_RATE.to_le_bytes());
    header[28..32].copy_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    header[32..34].copy_from_slice(&2_u16.to_le_bytes());
    header[34..36].copy_from_slice(&16_u16.to_le_bytes());
    header[36..40].copy_from_slice(b"data");
    header[40..44].copy_from_slice(&bytes.to_le_bytes());
    header
}

extern "C" fn stop_recording(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

fn record(library: &Library, id: &str) -> Result {
    let dir = library.directory(id)?;
    let _recording = lock(&dir.join(".recording"))?;
    if library.read(id)?.trashed {
        return Err("Restore this note before recording".into());
    }
    let memo_id = identifier();
    let partial = dir.join(format!("{memo_id}.part"));
    let mut file = private_file(&partial, true)?;
    file.write_all(&wav_header(0))?;
    // PulseAudio's native client follows PipeWire's default microphone. Only
    // this process owns the recorder, and every exit path reaps it.
    let mut child = match Command::new("parecord")
        .args([
            "--raw",
            "--format=s16le",
            "--rate=16000",
            "--channels=1",
            "--client-name=Seele Notes",
            "--stream-name=Voice memo",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            let _ = fs::remove_file(&partial);
            return Err(error.into());
        }
    };
    STOP.store(false, Ordering::Relaxed);
    unsafe {
        libc::signal(
            libc::SIGTERM,
            stop_recording as *const () as libc::sighandler_t,
        );
        libc::signal(
            libc::SIGINT,
            stop_recording as *const () as libc::sighandler_t,
        );
    }
    std::thread::spawn(|| {
        let mut line = String::new();
        let _ = io::stdin().read_line(&mut line);
        STOP.store(true, Ordering::Relaxed);
    });
    let result = (|| -> Result<u32> {
        let mut audio = child.stdout.take().ok_or("Recorder stdout unavailable")?;
        let fd = audio.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
        let (mut bytes, mut peak) = (0_u32, 0_f32);
        let mut last_emit = Instant::now();
        let mut last_audio = Instant::now();
        let mut stopping = None;
        let mut buffer = [0; 4096];
        emit(&json!({"recording":true}))?;
        loop {
            if (STOP.load(Ordering::Relaxed) || bytes >= MAX_AUDIO) && stopping.is_none() {
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGINT);
                }
                stopping = Some(Instant::now());
            }
            match audio.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let count = count.min((MAX_AUDIO - bytes) as usize);
                    file.write_all(&buffer[..count])?;
                    bytes += count as u32;
                    last_audio = Instant::now();
                    for sample in buffer[..count].chunks_exact(2) {
                        peak = peak.max(
                            (i16::from_le_bytes([sample[0], sample[1]]) as f32 / 32768.0)
                                .abs()
                                .sqrt(),
                        );
                    }
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(10))
                }
                Err(error) => return Err(error.into()),
            }
            if last_emit.elapsed() >= Duration::from_millis(50) {
                emit(
                    &json!({"level":peak,"duration":bytes as u64 * 1000 / (SAMPLE_RATE as u64 * 2)}),
                )?;
                last_emit = Instant::now();
                peak = 0.0;
            }
            if stopping.is_some_and(|time| time.elapsed() > Duration::from_secs(2)) {
                let _ = child.kill();
                break;
            }
            if last_audio.elapsed() > Duration::from_secs(10) {
                return Err("Microphone did not deliver audio".into());
            }
        }
        if stopping.is_none() {
            return Err("Microphone recording ended unexpectedly".into());
        }
        Ok(bytes - bytes % 2)
    })();
    let _ = child.kill();
    let _ = child.wait();
    match result {
        Ok(bytes) if bytes > 0 => {
            file.set_len(44 + bytes as u64)?;
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&wav_header(bytes))?;
            file.sync_all()?;
            fs::rename(&partial, dir.join(format!("{memo_id}.wav")))?;
            File::open(&dir)?.sync_all()?;
            emit(&json!({"saved":memo_id,"note":id}))?;
            Ok(())
        }
        Ok(_) => {
            fs::remove_file(partial)?;
            Err("No audio was recorded".into())
        }
        Err(error) => {
            let _ = fs::remove_file(partial);
            Err(error)
        }
    }
}

pub fn run(arguments: &[String]) -> Result {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".local/share"));
    let library = Library(data.join("seele-shell/notes"));
    private_dir(&library.0)?;
    match arguments.first().map(String::as_str).unwrap_or("watch") {
        "watch" => {
            let _writer = lock(&library.0.join(".writer"))?;
            let mut ready = library.list()?;
            ready["ready"] = json!(true);
            emit(&ready)?;
            for line in io::stdin().lock().lines() {
                let message: Value = match serde_json::from_str(&line?) {
                    Ok(value) => value,
                    Err(_) => {
                        emit(&json!({"ok":false,"error":"Invalid request"}))?;
                        continue;
                    }
                };
                let mut response = match library.request(&message) {
                    Ok(mut value) => {
                        value["ok"] = json!(true);
                        value
                    }
                    Err(error) => json!({"ok":false,"error":error.to_string()}),
                };
                response["request"] = message["request"].clone();
                emit(&response)?;
            }
            Ok(())
        }
        "record" => record(
            &library,
            arguments.get(1).ok_or("Note identifier required")?,
        ),
        _ => Err("Usage: seele-notes [watch|record ID]".into()),
    }
}
