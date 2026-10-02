//! Desktop lifecycle and narrowly scoped tailnet MCP tools. Remote callers
//! can request a write, but only a same-UID local control connection approves it.
mod http;
mod tools;
use crate::common::{self, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::Mutex,
};
use tokio_util::sync::CancellationToken;
const LIMIT: usize = 65536;
#[derive(Clone)]
pub struct Config {
    pub gateway: String,
    pub peer: String,
    pub port: u16,
    pub flake: PathBuf,
    pub services: Vec<String>,
    pub rebuild: bool,
}
impl Config {
    fn load() -> Result<Self> {
        let gateway =
            std::env::var("SEELE_HERMES_GATEWAY").unwrap_or_else(|_| "http://hermes:9119".into());
        let url = url::Url::parse(&gateway).map_err(|_| "Invalid Hermes gateway URL.")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("Invalid Hermes gateway URL.");
        }
        let peer = std::env::var("SEELE_HERMES_PEER").unwrap_or_else(|_| "hermes".into());
        if peer.is_empty()
            || peer.len() > 253
            || !peer
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c))
        {
            return Err("Invalid Hermes tailnet peer.");
        }
        let services = std::env::var("SEELE_HERMES_SERVICES")
            .unwrap_or_else(|_| "nix-daemon.service".into())
            .split(',')
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if services.len() > 32 || services.iter().any(|s| !tools::unit(s)) {
            return Err("Invalid Hermes service allowlist.");
        }
        let flake = PathBuf::from(
            std::env::var("SEELE_HERMES_FLAKE")
                .map_err(|_| "Configure the Hermes flake directory.")?,
        );
        if !flake.is_absolute() {
            return Err("The Hermes flake directory must be absolute.");
        }
        Ok(Self {
            gateway,
            peer,
            flake,
            services,
            port: std::env::var("SEELE_HERMES_PORT")
                .unwrap_or_else(|_| "8766".into())
                .parse()
                .map_err(|_| "Invalid Hermes MCP port.")?,
            rebuild: std::env::var("SEELE_HERMES_ALLOW_REBUILD").as_deref() == Ok("1"),
        })
    }
}
struct Pending {
    created: Instant,
    revision: String,
}
pub(super) struct State {
    report: Value,
    reported: Option<Instant>,
    reachable: bool,
    pending: BTreeMap<String, Pending>,
    outcomes: BTreeMap<String, (Instant, String)>,
}
impl State {
    fn new() -> Self {
        Self {
            report: json!({}),
            reported: None,
            reachable: false,
            pending: BTreeMap::new(),
            outcomes: BTreeMap::new(),
        }
    }
    fn expire(&mut self) {
        let expired = self
            .pending
            .iter()
            .filter(|(_, p)| p.created.elapsed() >= Duration::from_secs(120))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            self.pending.remove(&id);
            self.finish(&id, "expired");
        }
        self.outcomes
            .retain(|_, (at, _)| at.elapsed() < Duration::from_secs(300));
    }
    fn finish(&mut self, id: &str, outcome: &str) {
        if self.outcomes.len() >= 32 {
            if let Some(oldest) = self
                .outcomes
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(id, _)| id.clone())
            {
                self.outcomes.remove(&oldest);
            }
        }
        self.outcomes
            .insert(id.into(), (Instant::now(), outcome.into()));
    }
    fn snapshot(&mut self, config: &Config) -> Value {
        self.expire();
        let live = self
            .reported
            .is_some_and(|t| t.elapsed() < Duration::from_secs(15));
        let phase = if live {
            self.report["state"].as_str().unwrap_or("disconnected")
        } else {
            "disconnected"
        };
        json!({"version":1,"state":phase,"reachable":self.reachable,"desktop":live,"gateway":config.gateway,"session":if live { self.report["session"].as_str().unwrap_or("") } else { "" },"pending":self.pending.iter().map(|(id,p)| json!({"id":id,"revision":p.revision,"remaining":120u64.saturating_sub(p.created.elapsed().as_secs())})).collect::<Vec<_>>(),"allowRebuild":config.rebuild})
    }
    fn publish(&mut self, report: &Value) -> Result<()> {
        if !report.is_object()
            || report
                .as_object()
                .is_some_and(|m| m.keys().any(|k| !matches!(k.as_str(), "state" | "session")))
        {
            return Err("Invalid Hermes lifecycle report.");
        }
        let phase = report["state"]
            .as_str()
            .filter(|s| {
                matches!(
                    *s,
                    "disconnected" | "idle" | "listening" | "thinking" | "speaking"
                )
            })
            .ok_or("Invalid Hermes lifecycle state.")?;
        let session = report["session"].as_str().unwrap_or("");
        if session.len() > 128
            || !session
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
        {
            return Err("Invalid Hermes session identity.");
        }
        self.report = json!({"state":phase,"session":session});
        self.reported = Some(Instant::now());
        Ok(())
    }
}
pub(super) type Shared = Arc<Mutex<State>>;
fn socket_path() -> Result<PathBuf> {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .ok_or("Private runtime directory unavailable.")?;
    let info = fs::symlink_metadata(&root).map_err(|_| "Private runtime directory unavailable.")?;
    if !info.is_dir() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        return Err("Private runtime directory unavailable.");
    }
    let directory = root.join("seele-hermes");
    seele_runtime::fs::private_directory(&directory)
        .map_err(|_| "Private runtime directory unavailable.")?;
    let info =
        fs::symlink_metadata(&directory).map_err(|_| "Private runtime directory unavailable.")?;
    if !info.is_dir() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        return Err("Private runtime directory unavailable.");
    }
    Ok(directory.join("control.sock"))
}
async fn local(
    request: Value,
    shared: &Shared,
    config: &Config,
    cancel: CancellationToken,
) -> Result<Value> {
    match request["op"].as_str() {
        Some("snapshot") => Ok(shared.lock().await.snapshot(config)),
        Some("publish") => {
            shared.lock().await.publish(&request["report"])?;
            Ok(json!({"ok":true}))
        }
        Some("deny") => {
            let id = request["id"].as_str().ok_or("Invalid approval identity.")?;
            let mut state = shared.lock().await;
            state.expire();
            if state.pending.remove(id).is_none() {
                return Err("This rebuild request is no longer pending.");
            }
            state.finish(id, "denied");
            Ok(json!({"ok":true}))
        }
        Some("approve") => {
            if !config.rebuild {
                return Err("Rebuild permission is disabled.");
            }
            let id = request["id"].as_str().ok_or("Invalid approval identity.")?;
            let pending = {
                let mut state = shared.lock().await;
                state.expire();
                state
                    .pending
                    .remove(id)
                    .ok_or("This rebuild request is no longer pending.")?
            };
            // Consume once before any await; recheck the revision approved by the
            // user immediately before handing the fixed rebuild to a terminal.
            shared.lock().await.finish(id, "approval-consumed");
            if tools::revision(config, cancel.clone()).await? != pending.revision {
                shared.lock().await.finish(id, "stale-revision");
                return Err("The flake changed. Request and review a new rebuild.");
            }
            let mut command = Command::new("ghostty");
            command
                .args([
                    "--class=org.seele.hermes-rebuild",
                    "-e",
                    "seele-rebuild",
                    "os",
                    "switch",
                ])
                .arg(tools::immutable_flake(config, &pending.revision)?);
            if common::launch(command, Duration::from_secs(10), cancel)
                .await
                .is_err()
            {
                shared.lock().await.finish(id, "handoff-failed");
                return Err("Could not open the approved rebuild terminal.");
            }
            shared.lock().await.finish(id, "handed-off");
            Ok(json!({"ok":true,"state":"handed-off"}))
        }
        _ => Err("Unknown Hermes control operation."),
    }
}
async fn client(
    mut stream: UnixStream,
    state: Shared,
    config: Config,
    cancel: CancellationToken,
) -> std::io::Result<()> {
    if stream.peer_cred()?.uid() != unsafe { libc::geteuid() } {
        return Ok(());
    }
    let (read, mut write) = stream.split();
    let mut reader = BufReader::new(read);
    let Some(bytes) = tokio::time::timeout(
        Duration::from_secs(5),
        common::read_frame(&mut reader, LIMIT),
    )
    .await??
    else {
        return Ok(());
    };
    let request: Value = serde_json::from_slice(&bytes)?;
    if request["op"] == "watch" {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! { _=cancel.cancelled()=>break, _=interval.tick()=>{ common::write_json(&mut write, &state.lock().await.snapshot(&config)).await?; } }
        }
    } else {
        let reply = match local(request, &state, &config, cancel).await {
            Ok(value) => value,
            Err(error) => json!({"ok":false,"error":error}),
        };
        common::write_json(&mut write, &reply).await?;
    }
    Ok(())
}
pub async fn serve(cancel: CancellationToken) -> Result<()> {
    let config = Config::load()?;
    let path = socket_path()?;
    if path.exists() {
        if UnixStream::connect(&path).await.is_ok() {
            return Err("Hermes service is already running.");
        }
        fs::remove_file(&path).map_err(|_| "Could not retire the old Hermes socket.")?;
    }
    let listener =
        UnixListener::bind(&path).map_err(|_| "Could not open the private Hermes socket.")?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "Could not protect the Hermes socket.")?;
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let _cleanup = Cleanup(path);
    let state = Arc::new(Mutex::new(State::new()));
    let mut tasks = tokio::task::JoinSet::new();
    tasks.spawn(http::serve(config.clone(), state.clone(), cancel.clone()));
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| "Could not prepare Hermes health checks.")?;
    let poll_state = state.clone();
    let poll_url = format!("{}/api/status", config.gateway.trim_end_matches('/'));
    let poll_cancel = cancel.clone();
    tasks.spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(10));
        loop { tokio::select! {_=poll_cancel.cancelled()=>break, _=tick.tick()=>{
            let reachable = match http.get(&poll_url).send().await { Ok(r) if r.status().is_success()=>common::response_json(r).await.is_ok_and(|v| v["gateway_running"] == true), _=>false };
            poll_state.lock().await.reachable = reachable;
        }} } Ok(())
    });
    let slots = Arc::new(tokio::sync::Semaphore::new(16));
    let mut failed = false;
    loop {
        tokio::select! {
            _=cancel.cancelled()=>break,
            Some(done)=tasks.join_next()=>{ if !matches!(done,Ok(Ok(()))) { failed = true; cancel.cancel(); break; } },
            accepted=listener.accept()=>{
                let (stream,_) = accepted.map_err(|_| "Hermes control socket failed.")?;
                if let Ok(permit)=slots.clone().try_acquire_owned() { let s=state.clone();let c=config.clone();let stop=cancel.clone(); tasks.spawn(async move {let _permit=permit;let _=client(stream,s,c,stop).await;Ok(())}); }
            }
        }
    }
    cancel.cancel();
    while tasks.join_next().await.is_some() {}
    if failed {
        Err("Hermes listener failed; the service will retry.")
    } else {
        Ok(())
    }
}
pub async fn request(value: Value, watch: bool, cancel: CancellationToken) -> Result<()> {
    let mut stream = UnixStream::connect(socket_path()?)
        .await
        .map_err(|_| "Hermes service is unavailable.")?;
    if stream
        .peer_cred()
        .map_err(|_| "Hermes service is unavailable.")?
        .uid()
        != unsafe { libc::geteuid() }
    {
        return Err("Hermes service identity mismatch.");
    }
    stream
        .write_all(
            &seele_runtime::wire::json_frame(&value, LIMIT)
                .map_err(|_| "Hermes request is too large.")?,
        )
        .await
        .map_err(|_| "Hermes service is unavailable.")?;
    let mut reader = BufReader::new(stream);
    loop {
        tokio::select! { _=cancel.cancelled()=>break, result=tokio::time::timeout(Duration::from_secs(if watch { 5 } else { 45 }),common::read_frame(&mut reader,LIMIT))=>{
            let Some(bytes)=result.map_err(|_| "Hermes service is unavailable.")?.map_err(|_| "Hermes service is unavailable.")? else {break;};
            let value:Value=serde_json::from_slice(&bytes).map_err(|_| "Invalid Hermes response.")?;common::emit(&value).await.map_err(|_| "Hermes output failed.")?;
            if !watch {break;}
        }}
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn request_status_reports_denial_expiry_and_retirement_without_executing() {
        let config = Config {
            gateway: "http://hermes:9119".into(),
            peer: "hermes".into(),
            port: 8766,
            flake: "/tmp/flake".into(),
            services: vec!["nix-daemon.service".into()],
            rebuild: false,
        };
        let state = Arc::new(Mutex::new(State::new()));
        let id = uuid::Uuid::new_v4().to_string();
        let arguments = json!({"id":id});
        let read = || {
            tools::call(
                "rebuild_status",
                &arguments,
                &config,
                &state,
                CancellationToken::new(),
            )
        };
        // Requests use opaque identities, and absent status never implies success.
        assert_eq!(read().await.unwrap()["state"], "unknown-or-retired");
        state.lock().await.pending.insert(
            id.clone(),
            Pending {
                created: Instant::now(),
                revision: "fixture".into(),
            },
        );
        assert_eq!(read().await.unwrap()["state"], "awaiting-local-approval");
        local(
            json!({"op":"deny","id":id}),
            &state,
            &config,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(read().await.unwrap()["state"], "denied");
        state.lock().await.pending.insert(
            id.clone(),
            Pending {
                created: Instant::now() - Duration::from_secs(121),
                revision: "fixture".into(),
            },
        );
        assert_eq!(read().await.unwrap()["state"], "expired");
        state.lock().await.outcomes.insert(
            id.clone(),
            (
                Instant::now() - Duration::from_secs(301),
                "handed-off".into(),
            ),
        );
        assert_eq!(read().await.unwrap()["state"], "unknown-or-retired");
        let capabilities = tools::call(
            "capabilities",
            &json!({}),
            &config,
            &state,
            CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(capabilities["allowRebuild"], false);
        assert_eq!(capabilities["localApprovalRequired"], true);
        assert!(tools::call(
            "capabilities",
            &json!({"context":true}),
            &config,
            &state,
            CancellationToken::new()
        )
        .await
        .is_err());
    }
    #[test]
    fn lifecycle_is_metadata_only_and_expires() {
        let mut state = State::new();
        assert!(state
            .publish(&json!({"state":"thinking","session":"abc_123"}))
            .is_ok());
        assert!(state
            .publish(&json!({"state":"thinking","prompt":"secret"}))
            .is_err());
        assert!(state.publish(&json!({"state":"unknown"})).is_err());
        assert!(state
            .publish(&json!({"state":"idle","session":"bad\nname"}))
            .is_err());
        let config = Config {
            gateway: "http://hermes:9119".into(),
            peer: "hermes".into(),
            port: 8766,
            flake: "/tmp/flake".into(),
            services: vec![],
            rebuild: false,
        };
        assert_eq!(state.snapshot(&config)["state"], "thinking");
        state.reported = Some(Instant::now() - Duration::from_secs(16));
        assert_eq!(state.snapshot(&config)["state"], "disconnected");
    }
    #[tokio::test]
    async fn remote_cannot_approve_and_local_approval_is_single_use() {
        let config = Config {
            gateway: "http://hermes:9119".into(),
            peer: "hermes".into(),
            port: 8766,
            flake: "/tmp/flake".into(),
            services: vec![],
            rebuild: true,
        };
        let state = Arc::new(Mutex::new(State::new()));
        state.lock().await.pending.insert(
            "expired".into(),
            Pending {
                created: Instant::now() - Duration::from_secs(121),
                revision: "old".into(),
            },
        );
        assert!(local(
            json!({"op":"approve","id":"expired"}),
            &state,
            &config,
            CancellationToken::new()
        )
        .await
        .is_err());
        assert!(state.lock().await.pending.is_empty());
        assert!(!tools::catalog()
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "approve"));
    }
}
