//! Versioned private Unix IPC and the transfer CLI. Provider and lifecycle policy
//! live in separate modules; clients receive only sanitized metadata projections.
mod files;
mod model;
mod provider;
mod service;
use crate::common::{self, read_frame, write_json, Result};
use serde_json::{json, Value};
pub use service::Service;
use std::{
    fs::OpenOptions,
    os::{
        fd::AsRawFd,
        unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    },
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::Duration,
};
use tokio::{
    net::{UnixListener, UnixStream},
    sync::Semaphore,
};
use tokio_util::sync::CancellationToken;
const MAX_GROUPS: usize = 4096;
const MAX_HISTORY: usize = 8 * 1024 * 1024;
const MAX_REQUEST: usize = 256 * 1024;
fn socket_path() -> Result<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or("service-unavailable")?;
    let meta = dir.symlink_metadata().map_err(|_| "service-unavailable")?;
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o077 != 0 {
        return Err("unsafe-runtime-directory");
    }
    Ok(dir.join("seele-transfers.sock"))
}
struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        if let Ok(info) = self.path.symlink_metadata() {
            if info.dev() == self.device && info.ino() == self.inode {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}
pub async fn serve(cancel: CancellationToken) -> Result<()> {
    let path = socket_path()?;
    // A process-owned advisory lock prevents a second daemon removing a live socket.
    let lock_path = path.with_extension("lock");
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(lock_path)
        .map_err(|_| "service-already-running")?;
    let info = lock.metadata().map_err(|_| "service-already-running")?;
    if !info.is_file()
        || info.uid() != unsafe { libc::geteuid() }
        || info.mode() & 0o077 != 0
        || unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0
    {
        return Err("service-already-running");
    }
    if let Ok(info) = path.symlink_metadata() {
        if !info.file_type().is_socket() || info.uid() != unsafe { libc::geteuid() } {
            return Err("unsafe-socket");
        }
        if UnixStream::connect(&path).await.is_ok() {
            return Err("service-already-running");
        }
        std::fs::remove_file(&path).map_err(|_| "service-unavailable")?;
    }
    let listener = UnixListener::bind(&path).map_err(|_| "service-unavailable")?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .map_err(|_| "service-unavailable")?;
    let info = path.symlink_metadata().map_err(|_| "service-unavailable")?;
    let _guard = SocketGuard {
        path,
        device: info.dev(),
        inode: info.ino(),
    };
    let provider = provider::Provider::new(
        std::env::var_os("SEELE_TAILSCALE_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/run/tailscale/tailscaled.sock")),
    )?;
    let downloads = std::env::var_os("SEELE_TRANSFERS_DOWNLOADS")
        .map(PathBuf::from)
        .ok_or("destination-unavailable")?;
    let service = Service::new(
        provider,
        downloads,
        common::xdg("XDG_STATE_HOME", ".local/state").join("seele-transfers/history.json"),
        cancel.clone(),
    )?;
    let worker = service.clone();
    service.spawn(async move {
        worker.poll().await;
    });
    let worker = service.clone();
    service.spawn(async move {
        worker
            .provider
            .events(worker.clone(), worker.cancel.clone())
            .await;
    });
    let clients = Arc::new(Semaphore::new(32));
    loop {
        let stream = tokio::select! {r=listener.accept()=>r.map_err(|_|"service-unavailable")?.0,_=cancel.cancelled()=>break};
        if stream.peer_cred().map(|p| p.uid()).ok() != Some(unsafe { libc::geteuid() }) {
            continue;
        }
        let Ok(permit) = clients.clone().try_acquire_owned() else {
            continue;
        };
        let worker = service.clone();
        service.spawn(async move{let _permit=permit;let mut stream=tokio::io::BufReader::new(stream);
            let operation=async{
                let frame=tokio::time::timeout(Duration::from_secs(5),read_frame(&mut stream,MAX_REQUEST)).await.map_err(|_|"request-timeout")?.map_err(|_|"request-too-large")?.ok_or("invalid-request")?;
                let value=serde_json::from_slice(&frame).map_err(|_|"invalid-request")?;worker.dispatch(value).await
            };
            let result=tokio::select!{result=operation=>result,_=worker.cancel.cancelled()=>Err("service-unavailable")};
            let value=result.unwrap_or_else(|error|json!({"error":error}));let _=write_json(stream.get_mut(),&value).await;
        });
    }
    service.shutdown().await;
    Ok(())
}
pub async fn request(value: &Value) -> Result<Value> {
    tokio::time::timeout(Duration::from_secs(30), async {
        let path = socket_path()?;
        let info = path.symlink_metadata().map_err(|_| "service-unavailable")?;
        if !info.file_type().is_socket()
            || info.uid() != unsafe { libc::geteuid() }
            || info.mode() & 0o077 != 0
        {
            return Err("unsafe-socket");
        }
        let stream = UnixStream::connect(path)
            .await
            .map_err(|_| "service-unavailable")?;
        if stream.peer_cred().map(|p| p.uid()).ok() != Some(unsafe { libc::geteuid() }) {
            return Err("unsafe-socket");
        }
        let mut stream = tokio::io::BufReader::new(stream);
        write_json(stream.get_mut(), value)
            .await
            .map_err(|_| "service-unavailable")?;
        let bytes = read_frame(&mut stream, MAX_HISTORY + 1024 * 1024)
            .await
            .map_err(|_| "service-unavailable")?
            .ok_or("service-unavailable")?;
        serde_json::from_slice(&bytes).map_err(|_| "service-unavailable")
    })
    .await
    .map_err(|_| "service-unavailable")?
}
async fn open_panel(cancel: CancellationToken) -> Result<()> {
    let mut cmd = Command::new("seele-shellctl");
    cmd.arg("transfers");
    let _ = common::command(cmd, vec![], Duration::from_secs(10), 65536, cancel).await?;
    Ok(())
}
pub async fn run(args: &[String], cancel: CancellationToken) -> Result<()> {
    match args.first().map(String::as_str) {
        Some("serve") if args.len() == 1 => serve(cancel).await,
        Some("request") if args.len() == 1 => {
            let mut stdin =
                tokio::io::BufReader::new(common::FdIo::stdin().expect("stdin descriptor"));
            let bytes = read_frame(&mut stdin, MAX_REQUEST)
                .await
                .map_err(|_| "invalid-request")?
                .ok_or("invalid-request")?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid-request")?;
            let result = request(&value).await?;
            common::emit(&result)
                .await
                .map_err(|_| "service-unavailable")
        }
        Some("select") => {
            if args.len() > 1 {
                let result = request(&json!({"op":"select","paths":&args[1..]})).await?;
                if result.get("error").is_some() {
                    common::emit(&result)
                        .await
                        .map_err(|_| "service-unavailable")?;
                    return Err("invalid-selection");
                }
            }
            open_panel(cancel).await
        }
        Some("watch") if args.len() == 1 => {
            let mut previous = None;
            let mut heartbeat = std::time::Instant::now();
            loop {
                let query = json!({"op":"snapshot"});
                let result = tokio::select! {r=request(&query)=>r,_=cancel.cancelled()=>break};
                let value=result.unwrap_or_else(|_|json!({"version":1,"groups":[],"selection":[],"targets":[],"error":"service-unavailable"}));
                // Preserve a low-rate heartbeat for broken-pipe detection while
                // avoiding repeated QML parsing and rebinding of unchanged rows.
                if previous.as_ref() != Some(&value)
                    || heartbeat.elapsed() >= Duration::from_secs(20)
                {
                    if common::emit(&value).await.is_err() {
                        break;
                    }
                    previous = Some(value);
                    heartbeat = std::time::Instant::now();
                }
                tokio::select! {_=tokio::time::sleep(Duration::from_millis(500))=>{},_=cancel.cancelled()=>break};
            }
            Ok(())
        }
        _ => Err("invalid-operation"),
    }
}
