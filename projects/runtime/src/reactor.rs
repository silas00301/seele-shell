//! Optional reactor adapters for the same bounded native protocols.
use serde_json::Value;
use std::{io, time::Duration};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

pub async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    limit: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let buf = reader.fill_buf().await?;
        if buf.is_empty() {
            return Ok(if frame.is_empty() { None } else { Some(frame) });
        }
        let end = buf.iter().position(|b| *b == b'\n');
        let count = end.map_or(buf.len(), |n| n + 1);
        if count > limit.saturating_sub(frame.len()) {
            return Err(io::ErrorKind::InvalidData.into());
        }
        frame.extend_from_slice(&buf[..count]);
        reader.consume(count);
        if end.is_some() {
            return Ok(Some(frame));
        }
    }
}
pub async fn write_json<W: AsyncWrite + Unpin>(
    writer: &mut W,
    value: &Value,
    limit: usize,
    timeout: Duration,
) -> io::Result<()> {
    let bytes = crate::wire::json_frame(value, limit)?;
    tokio::time::timeout(timeout, async {
        writer.write_all(&bytes).await?;
        writer.flush().await
    })
    .await?
}

/// Reactor-owned stdin. Tokio's global stdin uses a blocking thread that cannot
/// be cancelled and would keep a signalled worker alive while its pipe is open.
pub struct FdIo {
    file: crate::wire::NonblockingFile,
    readiness: Option<tokio::io::unix::AsyncFd<std::os::fd::OwnedFd>>,
}
impl FdIo {
    pub fn stdin() -> io::Result<Self> {
        Self::duplicate(libc::STDIN_FILENO)
    }
    pub fn stdout() -> io::Result<Self> {
        Self::duplicate(libc::STDOUT_FILENO)
    }
    fn duplicate(source: libc::c_int) -> io::Result<Self> {
        use std::os::fd::FromRawFd;
        let fd = unsafe { libc::fcntl(source, libc::F_DUPFD_CLOEXEC, 3) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let file = crate::wire::NonblockingFile::new(unsafe { std::fs::File::from_raw_fd(fd) })?;
        if file.metadata()?.is_file() {
            return Ok(Self {
                file,
                readiness: None,
            });
        }
        let owned = file.try_clone()?.into();
        Ok(Self {
            file,
            readiness: Some(tokio::io::unix::AsyncFd::new(owned)?),
        })
    }
}
impl tokio::io::AsyncRead for FdIo {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        output: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        use std::{io::Read, os::fd::AsRawFd, task::Poll};
        if output.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        if let Some(readiness) = &self.readiness {
            loop {
                let mut ready = std::task::ready!(readiness.poll_read_ready(cx))?;
                match ready.try_io(|fd| {
                    let bytes = output.initialize_unfilled();
                    let size = unsafe {
                        libc::read(
                            fd.get_ref().as_raw_fd(),
                            bytes.as_mut_ptr().cast(),
                            bytes.len(),
                        )
                    };
                    if size < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    output.advance(size as usize);
                    Ok(())
                }) {
                    Ok(Err(error)) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Ok(result) => return Poll::Ready(result),
                    Err(_) => continue,
                }
            }
        }
        loop {
            match self.file.read(output.initialize_unfilled()) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Ok(size) => {
                    output.advance(size);
                    return Poll::Ready(Ok(()));
                }
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

impl tokio::io::AsyncWrite for FdIo {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        bytes: &[u8],
    ) -> std::task::Poll<io::Result<usize>> {
        use std::{io::Write, os::fd::AsRawFd, task::Poll};
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        if let Some(readiness) = &self.readiness {
            loop {
                let mut ready = std::task::ready!(readiness.poll_write_ready(cx))?;
                match ready.try_io(|fd| {
                    let size = unsafe {
                        libc::write(fd.get_ref().as_raw_fd(), bytes.as_ptr().cast(), bytes.len())
                    };
                    if size < 0 {
                        Err(io::Error::last_os_error())
                    } else {
                        Ok(size as usize)
                    }
                }) {
                    Ok(Err(error)) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Ok(result) => return Poll::Ready(result),
                    Err(_) => continue,
                }
            }
        }
        loop {
            match self.file.write(bytes) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return Poll::Ready(result),
            }
        }
    }
    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<io::Result<()>> {
        std::task::Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn descriptor_flags_restore_and_zero_length_io_does_not_wait() {
        use std::os::fd::AsRawFd;
        use tokio::io::{AsyncRead, AsyncWrite};
        let (stream, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let flags = unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) };
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let mut adapter = FdIo::duplicate(stream.as_raw_fd()).unwrap();
            assert_ne!(
                unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) } & libc::O_NONBLOCK,
                0
            );
            let mut output = tokio::io::ReadBuf::new(&mut []);
            let waker = std::task::Waker::noop();
            let mut context = std::task::Context::from_waker(waker);
            assert!(matches!(
                std::pin::Pin::new(&mut adapter).poll_read(&mut context, &mut output),
                std::task::Poll::Ready(Ok(()))
            ));
            assert!(matches!(
                std::pin::Pin::new(&mut adapter).poll_write(&mut context, &[]),
                std::task::Poll::Ready(Ok(0))
            ));
            drop(adapter);
            assert_eq!(
                unsafe { libc::fcntl(stream.as_raw_fd(), libc::F_GETFL) },
                flags
            );
            let directory = tempfile::tempdir().unwrap();
            let unsupported = std::fs::File::open(directory.path()).unwrap();
            let flags = unsafe { libc::fcntl(unsupported.as_raw_fd(), libc::F_GETFL) };
            assert!(FdIo::duplicate(unsupported.as_raw_fd()).is_err());
            assert_eq!(
                unsafe { libc::fcntl(unsupported.as_raw_fd(), libc::F_GETFL) },
                flags
            );
        });
    }

    #[test]
    fn reactor_frames_and_backpressure_are_bounded() {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .unwrap()
            .block_on(async {
                let input = b"12345\n".as_slice();
                let mut reader = tokio::io::BufReader::with_capacity(2, input);
                assert_eq!(
                    read_frame(&mut reader, 4).await.unwrap_err().kind(),
                    io::ErrorKind::InvalidData
                );
                let (mut writer, _reader) = tokio::io::duplex(8);
                let started = std::time::Instant::now();
                let result = write_json(
                    &mut writer,
                    &Value::String("x".repeat(64)),
                    128,
                    Duration::from_millis(25),
                )
                .await;
                assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
                assert!(started.elapsed() < Duration::from_millis(200));
            });
    }
}
