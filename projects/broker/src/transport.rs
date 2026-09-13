use crate::lifecycle::Broker;
use crate::{MAX_MESSAGE, MAX_REPLY};
use serde_json::{json, Value};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixListener as StdListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

pub fn inherited_listener(path: &Path) -> io::Result<UnixListener> {
    if std::env::var("LISTEN_PID").ok().as_deref() != Some(&std::process::id().to_string())
        || std::env::var("LISTEN_FDS").ok().as_deref() != Some("1")
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    // The inherited descriptor is transferred to this owner exactly once.
    let listener = unsafe { StdListener::from_raw_fd(3) };
    let fd = listener.as_raw_fd();
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut accepting: libc::c_int = 0;
    let mut length = std::mem::size_of_val(&accepting) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut accepting as *mut libc::c_int).cast(),
            &mut length,
        )
    } < 0
        || accepting != 1
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if listener.local_addr()?.as_pathname() != Some(path) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o600
        || metadata.mode() & libc::S_IFMT != libc::S_IFSOCK
    {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    listener.set_nonblocking(true)?;
    UnixListener::from_std(listener)
}
async fn connection(broker: Arc<Broker>, stream: UnixStream) {
    if stream
        .peer_cred()
        .ok()
        .is_none_or(|peer| peer.uid() != unsafe { libc::geteuid() })
    {
        return;
    }
    let mut reader = BufReader::new(stream);
    let read = tokio::time::timeout(
        Duration::from_secs(10),
        seele_runtime::reactor::read_frame(&mut reader, MAX_MESSAGE),
    )
    .await;
    let outcome = match read {
        Ok(Ok(Some(raw))) if raw.last() == Some(&b'\n') => match serde_json::from_slice(&raw) {
            Ok(message) => broker.call(message).await,
            Err(_) => Err("invalid_input"),
        },
        _ => Err("invalid_input"),
    };
    let mut reply = match outcome {
        Ok(mut value) => {
            value["ok"] = json!(true);
            value
        }
        Err(code) => json!({"ok":false,"error":code}),
    };
    reply["epoch"] = json!(broker.epoch);
    let _ = seele_runtime::reactor::write_json(
        reader.get_mut(),
        &reply,
        MAX_REPLY,
        Duration::from_secs(10),
    )
    .await;
}

pub async fn serve(
    listener: UnixListener,
    broker: Arc<Broker>,
    stop: Arc<AtomicUsize>,
    idle: Duration,
) -> io::Result<()> {
    let workers = broker.start();
    let permits = Arc::new(Semaphore::new(256));
    let mut clients = JoinSet::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    let result = loop {
        tokio::select! {
            accepted=listener.accept()=>match accepted {
                Ok((stream,_))=> {
                    let Ok(permit)=permits.clone().try_acquire_owned() else { drop(stream); continue; };
                    let broker=broker.clone(); clients.spawn(async move { let _permit=permit; connection(broker,stream).await });
                },
                Err(error)=>break Err(error),
            },
            _=clients.join_next(),if !clients.is_empty()=>(),
            _=ticker.tick()=>if stop.load(Ordering::Relaxed)!=0 || broker.idle(idle) { break Ok(()); },
        }
    };
    broker.close();
    clients.abort_all();
    while clients.join_next().await.is_some() {}
    for worker in workers {
        let _ = worker.await;
    }
    broker.clear();
    result
}
pub fn rpc(path: &Path, message: &Value, cancel: &AtomicUsize) -> Value {
    seele_runtime::wire::rpc(
        path,
        message,
        seele_runtime::wire::RpcLimits {
            timeout: Duration::from_secs(3600),
            request_bytes: MAX_MESSAGE,
            response_bytes: MAX_REPLY,
        },
        cancel,
    )
    .unwrap_or_else(|_| json!({"ok":false,"error":"broker_unavailable"}))
}
