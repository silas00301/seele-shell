//! Concurrent, bounded pipe I/O without per-pipe threads. Every child is owned
//! until reaped; errors, cancellation and deadlines kill its process group.
use crate::cancel::Cancellation;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::AtomicUsize;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub struct Limits {
    pub timeout: Duration,
    /// Combined stdout and stderr bytes, including unsuccessful commands.
    pub output: usize,
}

pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn termination_signal() -> io::Result<Arc<AtomicUsize>> {
    let signal = Arc::new(AtomicUsize::new(0));
    for number in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
        signal_hook::flag::register_usize(number, signal.clone(), number as usize)?;
    }
    Ok(signal)
}

struct OwnedChild {
    child: Child,
    reaped: bool,
    group: bool,
}
impl OwnedChild {
    fn finish(&mut self, detach_on_success: bool) -> io::Result<ExitStatus> {
        // Kill before reaping: the group leader PID cannot be recycled yet.
        let preserve = detach_on_success && self.succeeded()?;
        if !preserve {
            unsafe {
                libc::kill(
                    if self.group {
                        -(self.child.id() as libc::pid_t)
                    } else {
                        self.child.id() as libc::pid_t
                    },
                    libc::SIGKILL,
                );
            }
        }
        let status = self.child.wait()?;
        self.reaped = true;
        Ok(status)
    }
    fn succeeded(&self) -> io::Result<bool> {
        Ok(self.exit_info()?.is_some_and(|info| {
            info.si_code == libc::CLD_EXITED && unsafe { info.si_status() } == 0
        }))
    }
    fn exited(&self) -> io::Result<bool> {
        Ok(self.exit_info()?.is_some())
    }
    fn exit_info(&self) -> io::Result<Option<libc::siginfo_t>> {
        // Observe without reaping so cancellation never targets a reused PID.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                self.child.id(),
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(if unsafe { info.si_pid() } != 0 {
            Some(info)
        } else {
            None
        })
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.finish(false);
        }
    }
}

fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: the caller holds the stream owning this descriptor.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn capture(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<Output> {
    capture_with_stdout(command, input, limits, cancelled, |_| Ok(()))
}

/// Capture a trusted interactive program in the current foreground group.
/// Cancellation owns and reaps only the direct child. Use this only when the
/// program needs /dev/tty and its arguments disable spawning other commands.
pub fn capture_inherited_group(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<Output> {
    execute(
        command,
        Input::Bytes(input),
        limits,
        cancelled,
        CaptureMode {
            collect: true,
            retain_stdout: true,
            own_group: false,
            visible_stderr: false,
            detach_on_success: false,
        },
        |_| Ok(()),
    )
}

/// Feed an interactive picker and capture its selected value while its UI keeps
/// stderr and the current foreground terminal. Only the direct child is owned.
pub fn capture_interactive(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<Output> {
    execute(
        command,
        Input::Bytes(input),
        limits,
        cancelled,
        CaptureMode {
            collect: true,
            retain_stdout: true,
            own_group: false,
            visible_stderr: true,
            detach_on_success: false,
        },
        |_| Ok(()),
    )
}

/// Observe stdout chunks while the child is live. The observer runs before
/// cancellation is checked again, so session IDs already read are recoverable
/// even if a caller closes its surface during the same turn.
pub fn capture_with_stdout(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
    observe: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<Output> {
    execute(
        command,
        Input::Bytes(input),
        limits,
        cancelled,
        CaptureMode::CAPTURE,
        observe,
    )
}

/// Check exit status without reading diagnostics or authentication output.
pub fn discard(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<ExitStatus> {
    Ok(execute(
        command,
        Input::Bytes(input),
        limits,
        cancelled,
        CaptureMode {
            collect: false,
            retain_stdout: false,
            own_group: true,
            visible_stderr: false,
            detach_on_success: false,
        },
        |_| Ok(()),
    )?
    .status)
}

/// Run an intentional launcher (for example wl-copy/xdg-open). Its descendants
/// survive only a successful parent exit; errors and cancellation still clean
/// up the owned process group. Never use for finite worker commands.
pub fn discard_detaching(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<ExitStatus> {
    Ok(execute(
        command,
        Input::Bytes(input),
        limits,
        cancelled,
        CaptureMode::DETACH,
        |_| Ok(()),
    )?
    .status)
}

/// The same intentional launcher policy with descriptor-backed file input.
pub fn discard_file_detaching(
    command: &mut Command,
    input: &std::fs::File,
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<ExitStatus> {
    Ok(execute(
        command,
        Input::File(input),
        limits,
        cancelled,
        CaptureMode::DETACH,
        |_| Ok(()),
    )?
    .status)
}

/// Stream stdout with backpressure without retaining it. The observer must
/// enforce its own record bound; only stderr is retained under Limits::output.
pub fn stream_stdout(
    command: &mut Command,
    input: &[u8],
    limits: Limits,
    cancelled: &dyn Cancellation,
    observe: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<Output> {
    execute(
        command,
        Input::Bytes(input),
        limits,
        cancelled,
        CaptureMode {
            collect: true,
            retain_stdout: false,
            own_group: true,
            visible_stderr: false,
            detach_on_success: false,
        },
        observe,
    )
}

enum Input<'a> {
    Bytes(&'a [u8]),
    File(&'a std::fs::File),
}
struct CaptureMode {
    collect: bool,
    retain_stdout: bool,
    own_group: bool,
    visible_stderr: bool,
    detach_on_success: bool,
}
impl CaptureMode {
    const DETACH: Self = Self {
        collect: false,
        retain_stdout: false,
        own_group: true,
        visible_stderr: false,
        detach_on_success: true,
    };
    const CAPTURE: Self = Self {
        collect: true,
        retain_stdout: true,
        own_group: true,
        visible_stderr: false,
        detach_on_success: false,
    };
}

/// Feed an already-opened file directly to the child without copying its data
/// into memory. The child shares the file offset; callers may seek beforehand.
pub fn capture_file(
    command: &mut Command,
    input: &std::fs::File,
    limits: Limits,
    cancelled: &dyn Cancellation,
) -> io::Result<Output> {
    execute(
        command,
        Input::File(input),
        limits,
        cancelled,
        CaptureMode::CAPTURE,
        |_| Ok(()),
    )
}

/// Keep a private duplicated file descriptor across exec in this child only.
/// The Command owns the duplicate until dropped; the parent's CLOEXEC stays set.
pub fn inherit_file(command: &mut Command, file: &std::fs::File) -> io::Result<RawFd> {
    let owned = file.try_clone()?;
    let fd = owned.as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            let fd = owned.as_raw_fd();
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags < 0 || libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(fd)
}

fn execute(
    command: &mut Command,
    input: Input<'_>,
    limits: Limits,
    cancelled: &dyn Cancellation,
    mode: CaptureMode,
    mut observe: impl FnMut(&[u8]) -> io::Result<()>,
) -> io::Result<Output> {
    let CaptureMode {
        collect,
        retain_stdout,
        own_group,
        visible_stderr,
        detach_on_success,
    } = mode;
    let (input, stdin) = match input {
        Input::Bytes(bytes) => (bytes, Stdio::piped()),
        Input::File(file) => (&[][..], Stdio::from(file.try_clone()?)),
    };
    if cancelled.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let deadline = Instant::now()
        .checked_add(limits.timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut child = OwnedChild {
        child: command
            .stdin(stdin)
            .stdout(if collect {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stderr(if visible_stderr {
                Stdio::inherit()
            } else if collect {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .process_group(if own_group {
                0
            } else {
                unsafe { libc::getpgrp() }
            })
            .spawn()?,
        reaped: false,
        group: own_group,
    };
    let mut stdin = child.child.stdin.take();
    let mut stdout = child.child.stdout.take();
    let mut stderr = child.child.stderr.take();
    for fd in [
        stdin.as_ref().map(AsRawFd::as_raw_fd),
        stdout.as_ref().map(AsRawFd::as_raw_fd),
        stderr.as_ref().map(AsRawFd::as_raw_fd),
    ]
    .into_iter()
    .flatten()
    {
        nonblocking(fd)?;
    }
    let mut sent = 0;
    let mut out = Vec::new();
    let mut err = Vec::new();
    loop {
        if cancelled.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "command cancelled",
            ));
        }
        if sent == input.len() {
            stdin.take();
        }
        if stdout.is_none() && stderr.is_none() && child.exited()? {
            if detach_on_success && sent < input.len() && child.succeeded()? {
                // A launcher may fork before its child reads stdin. Continue
                // feeding that pipe before accepting a successful handoff.
                if stdin.is_none() {
                    return Err(io::ErrorKind::BrokenPipe.into());
                }
            } else {
                return Ok(Output {
                    status: child.finish(detach_on_success)?,
                    stdout: out,
                    stderr: err,
                });
            }
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "command deadline exceeded"))?;
        let mut polls = [
            libc::pollfd {
                fd: stdin.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLOUT,
                revents: 0,
            },
            libc::pollfd {
                fd: stdout.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: stderr.as_ref().map_or(-1, AsRawFd::as_raw_fd),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        // SAFETY: polls is an initialized array whose exact length is passed.
        if unsafe {
            libc::poll(
                polls.as_mut_ptr(),
                polls.len() as libc::nfds_t,
                remaining.as_millis().min(50) as i32,
            )
        } < 0
        {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if polls[0].revents != 0 {
            match stdin.as_mut().unwrap().write(&input[sent..]) {
                Ok(0) => {
                    stdin.take();
                }
                Ok(count) => sent += count,
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => {
                    stdin.take();
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => return Err(error),
            }
        }
        // One chunk per ready stream per turn prevents a noisy stream from
        // starving its sibling or cancellation. Read one byte past the budget
        // to reject an oversized stream without allocating past that budget.
        let mut chunk = [0u8; 16 * 1024];
        for (index, poll) in polls.iter().enumerate().skip(1) {
            if poll.revents == 0 {
                continue;
            }
            let room = if index == 1 && !retain_stdout {
                usize::MAX
            } else {
                limits.output.saturating_sub(out.len() + err.len())
            };
            let size = chunk.len().min(room.saturating_add(1));
            let result = if index == 1 {
                stdout.as_mut().unwrap().read(&mut chunk[..size])
            } else {
                stderr.as_mut().unwrap().read(&mut chunk[..size])
            };
            match result {
                Ok(0) => {
                    if index == 1 {
                        stdout.take();
                    } else {
                        stderr.take();
                    }
                }
                Ok(count) if count > room => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "command output exceeds its limit",
                    ))
                }
                Ok(count) => {
                    if index == 1 {
                        observe(&chunk[..count])?;
                        if retain_stdout {
                            out.extend_from_slice(&chunk[..count]);
                        }
                    } else {
                        err.extend_from_slice(&chunk[..count]);
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
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(script: &str, input: &[u8], output: usize, timeout: Duration) -> io::Result<Output> {
        capture(
            Command::new("sh").args(["-c", script]),
            input,
            Limits { timeout, output },
            &AtomicUsize::new(0),
        )
    }

    #[test]
    fn drains_both_pipes_while_writing_large_stdin() {
        let output = run(
            "printf diagnostics >&2; cat",
            &vec![b'a'; 1024 * 1024],
            2 * 1024 * 1024,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 1024 * 1024);
        assert_eq!(output.stderr, b"diagnostics");
    }

    #[test]
    fn rejects_output_floods_and_silent_descendants_holding_pipes() {
        assert_eq!(
            run("yes x", b"", 1024, Duration::from_secs(3))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let started = Instant::now();
        assert_eq!(
            run("sleep 30 & exit 0", b"", 1024, Duration::from_millis(50))
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn cancellation_terminates_and_reaps_the_child() {
        let root = tempfile::tempdir().unwrap();
        let pidfile = root.path().join("pid");
        let cancel = Arc::new(AtomicUsize::new(0));
        let signal = cancel.clone();
        let path = pidfile.clone();
        let stopper = std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(2);
            while !path.exists() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            signal.store(15, Ordering::Relaxed);
        });
        let result = capture(
            Command::new("sh")
                .args(["-c", "echo $$ > \"$1\"; exec sleep 30", "sh"])
                .arg(&pidfile),
            b"",
            Limits {
                timeout: Duration::from_secs(5),
                output: 1024,
            },
            &cancel,
        );
        stopper.join().unwrap();
        assert_eq!(result.err().unwrap().kind(), io::ErrorKind::Interrupted);
        let pid: i32 = std::fs::read_to_string(pidfile)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        // SAFETY: signal zero checks existence without delivering a signal.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    }
}

/// Run an interactive child with real inherited terminal streams, owning its
/// process group and returning the terminal to the caller on every exit path.
pub fn interactive(
    command: &mut Command,
    cancelled: &dyn Cancellation,
    timeout: Duration,
) -> io::Result<ExitStatus> {
    if cancelled.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut child = OwnedChild {
        child: command
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .process_group(0)
            .spawn()?,
        reaped: false,
        group: true,
    };
    let _terminal = Foreground::new(child.child.id())?;
    loop {
        if cancelled.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if child.exited()? {
            return child.finish(false);
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        std::thread::sleep(remaining.min(Duration::from_millis(25)));
    }
}

/// Forward merged stdout/stderr byte-for-byte while retaining only a tail.
/// Used by interactive rebuilds: stdin remains inherited and a foreground
/// terminal is handed to the owned child group until it exits.
pub fn tee(
    command: &mut Command,
    tail_bytes: usize,
    timeout: Duration,
    cancelled: &dyn Cancellation,
    writer: &mut std::fs::File,
) -> io::Result<(ExitStatus, Vec<u8>)> {
    use std::collections::VecDeque;
    use std::fs::File;
    use std::os::fd::FromRawFd;
    if cancelled.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    if tail_bytes > 16 * 1024 * 1024 {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or(io::ErrorKind::InvalidInput)?;
    let mut descriptors = [-1; 2];
    // SAFETY: pipe initializes exactly two descriptors in caller-owned memory.
    #[cfg(target_os = "linux")]
    let result = unsafe { libc::pipe2(descriptors.as_mut_ptr(), libc::O_CLOEXEC) };
    #[cfg(not(target_os = "linux"))]
    let result = unsafe { libc::pipe(descriptors.as_mut_ptr()) };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    let mut read = unsafe { File::from_raw_fd(descriptors[0]) };
    let write = unsafe { File::from_raw_fd(descriptors[1]) };
    for descriptor in descriptors {
        if unsafe { libc::fcntl(descriptor, libc::F_SETFD, libc::FD_CLOEXEC) } != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    nonblocking(read.as_raw_fd())?;
    let mut child = OwnedChild {
        child: command
            .stdin(Stdio::inherit())
            .stdout(write.try_clone()?)
            .stderr(write)
            .process_group(0)
            .spawn()?,
        reaped: false,
        group: true,
    };
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let _terminal = Foreground::new(child.child.id())?;
    let mut output = crate::wire::NonblockingFile::new(writer.try_clone()?)?;
    let mut tail = VecDeque::with_capacity(tail_bytes);
    let mut buffer = [0u8; 16 * 1024];
    let mut eof = false;
    loop {
        if cancelled.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        if eof && child.exited()? {
            return Ok((child.finish(false)?, tail.into()));
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(io::ErrorKind::TimedOut)?;
        let mut descriptor = libc::pollfd {
            fd: if eof { -1 } else { read.as_raw_fd() },
            events: libc::POLLIN,
            revents: 0,
        };
        let polled =
            unsafe { libc::poll(&mut descriptor, 1, remaining.as_millis().min(50) as i32) };
        if polled < 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error);
        }
        if polled == 0 {
            continue;
        }
        match read.read(&mut buffer) {
            Ok(0) => eof = true,
            Ok(count) => {
                crate::wire::write_bytes(
                    &mut output,
                    &buffer[..count],
                    deadline.saturating_duration_since(Instant::now()),
                    cancelled,
                )?;
                output.flush()?;
                let keep = count.min(tail_bytes);
                let remove = (tail.len() + keep).saturating_sub(tail_bytes);
                tail.drain(..remove);
                tail.extend(&buffer[count - keep..count]);
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

struct Foreground(Option<libc::pid_t>);
impl Foreground {
    fn new(child: u32) -> io::Result<Self> {
        // SAFETY: tcgetpgrp simply fails for pipes/non-controlling terminals.
        let foreground = unsafe { libc::tcgetpgrp(libc::STDIN_FILENO) };
        if foreground < 0 || foreground != unsafe { libc::getpgrp() } {
            return Ok(Self(None));
        }
        foreground_group(child as libc::pid_t)?;
        unsafe {
            libc::kill(-(child as libc::pid_t), libc::SIGCONT);
        }
        Ok(Self(Some(foreground)))
    }
}
impl Drop for Foreground {
    fn drop(&mut self) {
        if let Some(group) = self.0 {
            let _ = foreground_group(group);
        }
    }
}
fn foreground_group(group: libc::pid_t) -> io::Result<()> {
    // Block SIGTTOU in this thread while handing the terminal back. Changing
    // process-wide signal disposition would race unrelated worker threads.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        let mut old: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTTOU);
        let status = libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old);
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status));
        }
        let status = libc::tcsetpgrp(libc::STDIN_FILENO, group);
        let error = io::Error::last_os_error();
        libc::pthread_sigmask(libc::SIG_SETMASK, &old, std::ptr::null_mut());
        if status != 0 {
            return Err(error);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tee_tests {
    use super::*;
    #[test]
    fn tee_preserves_raw_progress_and_only_retains_a_bounded_tail() {
        use std::io::{Seek, SeekFrom};
        let mut file = tempfile::tempfile().unwrap();
        let (status, tail) = tee(
            Command::new("sh").args([
                "-c",
                r"printf '\033[31mstep\r'; printf '\033[Kdone\n' >&2; exit 7",
            ]),
            5,
            Duration::from_secs(2),
            &AtomicUsize::new(0),
            &mut file,
        )
        .unwrap();
        assert_eq!(status.code(), Some(7));
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"\x1b[31mstep\r\x1b[Kdone\n");
        assert_eq!(tail, b"done\n");
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn file_stdin_streams_and_descriptor_inheritance_is_child_only() {
        use std::io::{Seek, SeekFrom};
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(b"exact private bytes").unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        let limits = Limits {
            timeout: Duration::from_secs(2),
            output: 128,
        };
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let output = capture_file(&mut Command::new("cat"), &file, limits, &cancel).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"exact private bytes");
        #[cfg(target_os = "linux")]
        {
            file.seek(SeekFrom::Start(0)).unwrap();
            let mut command = Command::new("sh");
            let fd = inherit_file(&mut command, &file).unwrap();
            assert_ne!(
                unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
                0
            );
            command
                .args(["-c", "cat /proc/self/fd/\"$1\"", "sh"])
                .arg(fd.to_string());
            let output = capture(&mut command, b"", limits, &cancel).unwrap();
            assert!(output.status.success());
            assert_eq!(output.stdout, b"exact private bytes");
            assert_ne!(
                unsafe { libc::fcntl(fd, libc::F_GETFD) } & libc::FD_CLOEXEC,
                0
            );
        }
    }
    #[test]
    fn inherited_picker_group_is_unchanged() {
        let output = capture_interactive(
            Command::new("sh").args(["-c", "ps -o pgid= -p $$"]),
            b"",
            Limits {
                timeout: Duration::from_secs(2),
                output: 128,
            },
            &AtomicUsize::new(0),
        )
        .unwrap();
        assert!(output.status.success());
        let group: libc::pid_t = std::str::from_utf8(&output.stdout)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(group, unsafe { libc::getpgrp() });
    }
}
