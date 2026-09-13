use seele_maintenance::{
    model::{self, Inbox},
    publishers,
    service::{Service, SocketBroker},
    ProcessExecutor, LIMIT, SNAPSHOT_LIMIT,
};
use serde_json::{json, Value};
use std::{
    io::{self, Read},
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    sync::{atomic::Ordering, mpsc, Arc, Mutex},
    thread,
    time::Duration,
};

fn runtime_socket() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
        .join("seele-maintenance.sock")
}
fn home_path(suffix: &str) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
        .join(suffix)
}
fn connection(service: &Arc<Service>, stream: UnixStream) {
    let reply = (|| -> io::Result<Value> {
        if !seele_runtime::wire::same_uid(&stream)? {
            return Err(io::ErrorKind::PermissionDenied.into());
        }
        let message =
            seele_runtime::wire::receive(&stream, LIMIT, Duration::from_secs(5), &service.stop)?;
        service
            .call(&message)
            .map(|value| {
                let mut reply = value;
                reply["ok"] = json!(true);
                reply
            })
            .map_err(|_| io::ErrorKind::InvalidInput.into())
    })()
    .unwrap_or_else(|_| json!({"ok":false,"error":"Request failed; refresh and retry"}));
    let _ = seele_runtime::wire::send(
        &stream,
        &reply,
        SNAPSHOT_LIMIT,
        Duration::from_secs(5),
        &service.stop,
    );
}
fn listener() -> io::Result<UnixListener> {
    if std::env::var("LISTEN_PID").ok().as_deref() != Some(std::process::id().to_string().as_str())
        || std::env::var("LISTEN_FDS").ok().as_deref() != Some("1")
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "maintenance requires its managed user socket",
        ));
    }
    // Validate inherited descriptor before transferring ownership. Reject other
    // address families, non-listening descriptors and accidental descriptors.
    let mut socket_type: libc::c_int = 0;
    let mut length = std::mem::size_of_val(&socket_type) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            3,
            libc::SOL_SOCKET,
            libc::SO_TYPE,
            (&mut socket_type as *mut libc::c_int).cast(),
            &mut length,
        )
    } != 0
        || socket_type != libc::SOCK_STREAM
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut accepting: libc::c_int = 0;
    if unsafe {
        libc::getsockopt(
            3,
            libc::SOL_SOCKET,
            libc::SO_ACCEPTCONN,
            (&mut accepting as *mut libc::c_int).cast(),
            &mut length,
        )
    } != 0
        || accepting != 1
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut address: libc::sockaddr_storage = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&address) as libc::socklen_t;
    if unsafe {
        libc::getsockname(
            3,
            (&mut address as *mut libc::sockaddr_storage).cast(),
            &mut length,
        )
    } != 0
        || address.ss_family != libc::AF_UNIX as libc::sa_family_t
    {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    if unsafe { libc::fcntl(3, libc::F_SETFD, libc::FD_CLOEXEC) } == -1 {
        return Err(io::Error::last_os_error());
    }
    let listener = unsafe { UnixListener::from_raw_fd(3) };
    listener.set_nonblocking(true)?;
    Ok(listener)
}
fn main() {
    if run().is_err() {
        eprintln!("Maintenance unavailable; verify the managed configuration and socket.");
        std::process::exit(1);
    }
}
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let mode = arguments.next().ok_or("mode required")?;
    if !["serve", "request"].contains(&mode.as_str()) {
        return Err("invalid mode".into());
    }
    let mut config_path = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_path(".config"))
        .join("seele-maintenance/config.json");
    let mut state_path = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_path(".local/state"))
        .join("seele-maintenance/inbox.json");
    let mut socket_path = runtime_socket();
    while let Some(argument) = arguments.next() {
        let value = arguments.next().ok_or("missing argument")?;
        match argument.as_str() {
            "--config" => config_path = value.into(),
            "--state" => state_path = value.into(),
            "--socket" => socket_path = value.into(),
            _ => return Err("invalid argument".into()),
        }
    }
    let stop = seele_runtime::process::termination_signal()?;
    if mode == "request" {
        let reply = (|| -> Result<Value, Box<dyn std::error::Error>> {
            let mut bytes = vec![];
            io::stdin().take(LIMIT as u64 + 1).read_to_end(&mut bytes)?;
            if bytes.len() > LIMIT {
                return Err("invalid request".into());
            }
            let request: Value = serde_json::from_slice(&bytes)?;
            Ok(seele_runtime::wire::rpc(
                &socket_path,
                &request,
                seele_runtime::wire::RpcLimits {
                    timeout: Duration::from_secs(10),
                    request_bytes: LIMIT,
                    response_bytes: SNAPSHOT_LIMIT,
                },
                &stop,
            )
            .unwrap_or_else(|_| json!({"ok":false,"error":"service_unavailable"})))
        })()
        .unwrap_or_else(|_| json!({"ok":false,"error":"Invalid request"}));
        serde_json::to_writer(io::stdout().lock(), &reply)?;
        println!();
        return Ok(());
    }
    let listener = listener()?;
    let config: Value = serde_json::from_slice(&seele_maintenance::read_bounded(
        &config_path,
        LIMIT,
        false,
    )?)?;
    publishers::validate_config(&config)?;
    let inbox = Inbox::new(
        model::registrations(&config),
        Some(state_path),
        model::now(),
    )?;
    let broker_path = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/nonexistent"))
        .join("seele-codex.sock");
    let service = Arc::new(Service::new(
        config,
        inbox,
        stop.clone(),
        Arc::new(ProcessExecutor {
            cancelled: stop.clone(),
        }),
        Arc::new(SocketBroker {
            path: broker_path,
            stop: stop.clone(),
        }),
        Arc::new(publishers::collect),
    ));
    let schedules = service.schedules();
    // Fixed worker and queue counts stop a same-UID slow client from allocating
    // unbounded threads, descriptors or in-flight JSON snapshots.
    let (sender, receiver) = mpsc::sync_channel::<UnixStream>(8);
    let receiver = Arc::new(Mutex::new(receiver));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let service = service.clone();
            let receiver = receiver.clone();
            thread::spawn(move || loop {
                let next = receiver.lock().unwrap().recv();
                let Ok(stream) = next else { break };
                if service.stop.load(Ordering::Relaxed) == 0 {
                    connection(&service, stream);
                }
            })
        })
        .collect();
    while stop.load(Ordering::Relaxed) == 0 {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = sender.try_send(stream);
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                let mut poll = libc::pollfd {
                    fd: listener.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                };
                unsafe {
                    libc::poll(&mut poll, 1, 100);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => (),
            Err(_) => {
                stop.store(1, Ordering::Relaxed);
            }
        }
    }
    drop(listener);
    drop(sender);
    for worker in workers {
        let _ = worker.join();
    }
    service.close();
    for schedule in schedules {
        let _ = schedule.join();
    }
    Ok(())
}
