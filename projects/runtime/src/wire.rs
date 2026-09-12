//! Bounded newline framing, without allocating an attacker-selected line size.
use crate::cancel::Cancellation;
use std::io::{self, BufRead};

pub fn read_frame<R: BufRead>(
    reader: &mut R,
    frame: &mut Vec<u8>,
    limit: usize,
) -> io::Result<bool> {
    frame.clear();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(!frame.is_empty());
        }
        let end = available.iter().position(|byte| *byte == b'\n');
        let count = end.map_or(available.len(), |offset| offset + 1);
        if count > limit.saturating_sub(frame.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message exceeds its limit",
            ));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_descriptor_input_handles_eof_limit_timeout_and_cancel() {
        use std::os::unix::net::UnixStream;
        let cancel = std::sync::atomic::AtomicUsize::new(0);
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        writer.write_all(b"abc").unwrap();
        drop(writer);
        assert_eq!(
            read_bytes(&mut reader, 3, Duration::from_secs(1), &cancel).unwrap(),
            b"abc"
        );
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        writer.write_all(b"abcd").unwrap();
        assert_eq!(
            read_bytes(&mut reader, 3, Duration::from_secs(1), &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let (mut reader, _writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        assert_eq!(
            read_bytes(&mut reader, 3, Duration::from_millis(20), &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::TimedOut
        );
        cancel.store(1, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            read_bytes(&mut reader, 3, Duration::from_secs(1), &cancel)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn fragmented_frames_eof_and_exact_limits() {
        let mut reader = io::BufReader::with_capacity(2, &b"123\n45"[..]);
        let mut frame = Vec::new();
        assert!(read_frame(&mut reader, &mut frame, 4).unwrap());
        assert_eq!(frame, b"123\n");
        assert!(read_frame(&mut reader, &mut frame, 4).unwrap());
        assert_eq!(frame, b"45");
        assert!(!read_frame(&mut reader, &mut frame, 4).unwrap());
        assert!(read_frame(&mut &b"12345\n"[..], &mut frame, 4).is_err());
        assert!(frame.len() <= 4);
    }
}

use serde_json::Value;
use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct RpcLimits {
    pub timeout: Duration,
    pub request_bytes: usize,
    pub response_bytes: usize,
}

/// Serialize directly into a bounded buffer, including the trailing newline.
/// Escaping can expand text; the bound applies during serialization, before an
/// untrusted object can cause a larger allocation.
pub fn json_frame(message: &Value, limit: usize) -> io::Result<Vec<u8>> {
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            let needed = self
                .bytes
                .len()
                .checked_add(bytes.len())
                .filter(|needed| *needed <= self.limit)
                .ok_or(io::ErrorKind::InvalidInput)?;
            if needed > self.bytes.capacity() {
                let capacity = needed
                    .max(self.bytes.capacity().saturating_mul(2))
                    .min(self.limit);
                self.bytes
                    .try_reserve_exact(capacity - self.bytes.len())
                    .map_err(|_| io::ErrorKind::OutOfMemory)?;
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded {
        bytes: Vec::with_capacity(limit.min(8192)),
        limit,
    };
    serde_json::to_writer(&mut output, message).map_err(|_| io::ErrorKind::InvalidInput)?;
    output.write_all(b"\n")?;
    Ok(output.bytes)
}

/// Authenticate both directions of a private Unix-socket exchange.
pub fn same_uid(stream: &UnixStream) -> io::Result<bool> {
    #[cfg(target_os = "linux")]
    {
        let mut credentials: libc::ucred = unsafe { std::mem::zeroed() };
        let mut length = std::mem::size_of_val(&credentials) as libc::socklen_t;
        // SAFETY: getsockopt receives an initialized buffer and its exact size.
        if unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut credentials as *mut libc::ucred).cast(),
                &mut length,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(length as usize == std::mem::size_of::<libc::ucred>()
            && credentials.uid == unsafe { libc::geteuid() })
    }
    #[cfg(not(target_os = "linux"))]
    {
        let (mut uid, mut gid) = (0, 0);
        // SAFETY: live stream descriptor and writable uid/gid pointers.
        if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(uid == unsafe { libc::geteuid() })
    }
}

fn ready(
    fd: libc::c_int,
    events: libc::c_short,
    deadline: Instant,
    cancel: &dyn Cancellation,
) -> io::Result<()> {
    loop {
        if cancel.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        let mut descriptor = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        // SAFETY: one initialized pollfd remains live throughout the call.
        let result =
            unsafe { libc::poll(&mut descriptor, 1, remaining.as_millis().min(50) as i32) };
        if result > 0 {
            return Ok(());
        }
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}

/// One JSON request/response over an authenticated Unix stream. Limits apply to
/// total wall time, serialized request and response (including final newline).
/// The nonblocking connect cannot hang behind a full server backlog.
pub fn rpc(
    path: &Path,
    message: &Value,
    limits: RpcLimits,
    cancel: &dyn Cancellation,
) -> io::Result<Value> {
    let deadline = Instant::now()
        .checked_add(limits.timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let bytes = json_frame(message, limits.request_bytes)?;
    let socket = socket2::Socket::new(socket2::Domain::UNIX, socket2::Type::STREAM, None)?;
    socket.set_nonblocking(true)?;
    if let Err(error) = socket.connect(&socket2::SockAddr::unix(path)?) {
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error);
        }
        ready(socket.as_raw_fd(), libc::POLLOUT, deadline, cancel)?;
        if let Some(error) = socket.take_error()? {
            return Err(error);
        }
    }
    let descriptor: std::os::fd::OwnedFd = socket.into();
    let mut stream = UnixStream::from(descriptor);
    if !same_uid(&stream)? {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    write_bytes(
        &mut stream,
        &bytes,
        deadline.saturating_duration_since(Instant::now()),
        cancel,
    )?;
    receive_until(&stream, limits.response_bytes, deadline, cancel)
}

#[cfg(test)]
mod socket_tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    #[test]
    fn authenticates_peer_and_handles_fragmented_reply() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("service.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            assert!(same_uid(&stream).unwrap());
            let mut frame = Vec::new();
            read_frame(&mut io::BufReader::new(&stream), &mut frame, 128).unwrap();
            assert_eq!(
                serde_json::from_slice::<Value>(&frame).unwrap()["op"],
                "list"
            );
            for part in [b"{\"ok\":".as_slice(), b"true}\n"] {
                stream.write_all(part).unwrap();
            }
        });
        let result = rpc(
            &path,
            &serde_json::json!({"op":"list"}),
            RpcLimits {
                timeout: Duration::from_secs(1),
                request_bytes: 128,
                response_bytes: 128,
            },
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert_eq!(result["ok"], true);
        server.join().unwrap();
    }
    #[test]
    fn silent_peer_cannot_hold_the_client_forever() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("silent.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        let result = rpc(
            &path,
            &Value::Null,
            RpcLimits {
                timeout: Duration::from_millis(30),
                request_bytes: 128,
                response_bytes: 128,
            },
            &AtomicUsize::new(0),
        );
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::TimedOut);
        server.join().unwrap();
    }
}

/// Write to a nonblocking pipe/socket under one absolute deadline.
/// Callers own descriptor nonblocking configuration to avoid changing aliases.
pub fn write_bytes<W: Write + AsFd>(
    stream: &mut W,
    bytes: &[u8],
    timeout: Duration,
    cancel: &dyn Cancellation,
) -> io::Result<()> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut written = 0;
    while written < bytes.len() {
        ready(stream.as_fd().as_raw_fd(), libc::POLLOUT, deadline, cancel)?;
        match stream.write(&bytes[written..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(count) => written += count,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Read to EOF from a nonblocking descriptor with one absolute deadline and
/// byte limit. Useful for subprocess hook payloads which need no newline.
pub fn read_bytes<R: Read + AsFd>(
    reader: &mut R,
    limit: usize,
    timeout: Duration,
    cancel: &dyn Cancellation,
) -> io::Result<Vec<u8>> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        ready(reader.as_fd().as_raw_fd(), libc::POLLIN, deadline, cancel)?;
        let room = limit.saturating_sub(bytes.len());
        let size = chunk.len().min(room.saturating_add(1));
        match reader.read(&mut chunk[..size]) {
            Ok(0) => return Ok(bytes),
            Ok(count) if count <= room => bytes.extend_from_slice(&chunk[..count]),
            Ok(_) => return Err(io::ErrorKind::InvalidData.into()),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
}

/// Send one JSON object to a nonblocking same-user IPC connection.
pub fn send(
    stream: &UnixStream,
    message: &Value,
    limit: usize,
    timeout: Duration,
    cancel: &dyn Cancellation,
) -> io::Result<()> {
    let bytes = json_frame(message, limit)?;
    stream.set_nonblocking(true)?;
    write_bytes(&mut &*stream, &bytes, timeout, cancel)
}

/// Receive one JSON frame with a wall-clock deadline, including slow-drip peers.
pub fn receive(
    stream: &UnixStream,
    limit: usize,
    timeout: Duration,
    cancel: &dyn Cancellation,
) -> io::Result<Value> {
    stream.set_nonblocking(true)?;
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    receive_until(stream, limit, deadline, cancel)
}

fn receive_until(
    mut stream: &UnixStream,
    limit: usize,
    deadline: Instant,
    cancel: &dyn Cancellation,
) -> io::Result<Value> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        ready(stream.as_raw_fd(), libc::POLLIN, deadline, cancel)?;
        let room = limit.saturating_sub(bytes.len());
        let size = chunk.len().min(room.saturating_add(1));
        match stream.read(&mut chunk[..size]) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                let newline = chunk[..count].iter().position(|byte| *byte == b'\n');
                let used = newline.map_or(count, |offset| offset + 1);
                if used > room {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                bytes.extend_from_slice(&chunk[..used]);
                if newline.is_some() {
                    return serde_json::from_slice(&bytes).map_err(Into::into);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
    }
}

/// Own a temporary nonblocking mode and restore the original flags before
/// closing. The caller must serialize access to aliases of the same open-file
/// description; duplicated descriptors share these flags, including across fork.
pub struct NonblockingFile {
    file: std::fs::File,
    flags: libc::c_int,
}
impl NonblockingFile {
    pub fn new(file: std::fs::File) -> io::Result<Self> {
        let fd = file.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        let owned = Self { file, flags };
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(owned)
    }
}
impl Drop for NonblockingFile {
    fn drop(&mut self) {
        unsafe {
            libc::fcntl(self.file.as_raw_fd(), libc::F_SETFL, self.flags);
        }
    }
}
impl std::ops::Deref for NonblockingFile {
    type Target = std::fs::File;
    fn deref(&self) -> &Self::Target {
        &self.file
    }
}
impl AsRawFd for NonblockingFile {
    fn as_raw_fd(&self) -> libc::c_int {
        self.file.as_raw_fd()
    }
}
impl AsFd for NonblockingFile {
    fn as_fd(&self) -> std::os::fd::BorrowedFd<'_> {
        self.file.as_fd()
    }
}
impl Read for NonblockingFile {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.file.read(bytes)
    }
}
impl Write for NonblockingFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Own a nonblocking stdout duplicate for cancellable protocol output. Scope
/// this owner around the full output loop, or serialize creation/write/drop.
pub fn nonblocking_stdout() -> io::Result<NonblockingFile> {
    use std::os::fd::FromRawFd;
    let descriptor = unsafe { libc::fcntl(libc::STDOUT_FILENO, libc::F_DUPFD_CLOEXEC, 3) };
    if descriptor < 0 {
        return Err(io::Error::last_os_error());
    }
    NonblockingFile::new(unsafe { std::fs::File::from_raw_fd(descriptor) })
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn nonblocking_owner_restores_shared_flags_on_success_and_error() {
        use std::os::fd::FromRawFd;
        let (stream, _peer) = UnixStream::pair().unwrap();
        let initial = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) };
        assert_eq!(initial & libc::O_NONBLOCK, 0);
        let duplicate = || {
            let fd = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_DUPFD_CLOEXEC, 3) };
            assert!(fd >= 0);
            unsafe { std::fs::File::from_raw_fd(fd) }
        };
        let scope = || -> io::Result<()> {
            let _owner = NonblockingFile::new(duplicate())?;
            assert_ne!(
                unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
                0
            );
            Err(io::ErrorKind::BrokenPipe.into())
        };
        assert_eq!(scope().unwrap_err().kind(), io::ErrorKind::BrokenPipe);
        assert_eq!(
            unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) },
            initial
        );
        stream.set_nonblocking(true).unwrap();
        drop(NonblockingFile::new(duplicate()).unwrap());
        assert_ne!(
            unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
            0
        );
    }

    #[test]
    fn serialization_bounds_apply_after_json_escaping() {
        let message = Value::String("\0".repeat(1_000_000));
        assert_eq!(
            json_frame(&message, 64).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
        let frame = json_frame(&Value::Null, 5).unwrap();
        assert_eq!(frame, b"null\n");
        assert!(frame.capacity() <= 5);
        assert!(json_frame(&Value::Null, 4).is_err());
    }
    #[test]
    fn slow_drip_and_output_floods_cannot_extend_deadlines_or_buffers() {
        let (client, mut peer) = UnixStream::pair().unwrap();
        let sender = std::thread::spawn(move || {
            for _ in 0..10 {
                if peer.write_all(b" ").is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
        });
        let started = Instant::now();
        let result = receive(&client, 64, Duration::from_millis(40), &AtomicUsize::new(0));
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_millis(150));
        drop(client);
        sender.join().unwrap();
        let (client, mut peer) = UnixStream::pair().unwrap();
        peer.write_all(b"12345678").unwrap();
        assert_eq!(
            receive(&client, 4, Duration::from_secs(1), &AtomicUsize::new(0))
                .unwrap_err()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let stop = std::sync::atomic::AtomicBool::new(true);
        assert_eq!(
            receive(&client, 4, Duration::from_secs(1), &stop)
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }
}
