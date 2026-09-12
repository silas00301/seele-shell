//! Bounded transfer jobs and explicit desktop actions over the provider contract.
use super::{files, model::*, open_panel, provider, MAX_GROUPS, MAX_HISTORY};
use crate::common::{self, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    fs::OpenOptions,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
pub struct Service {
    pub(super) state: Mutex<State>,
    pub(super) provider: provider::Provider,
    pub(super) directory: PathBuf,
    pub(super) history: PathBuf,
    pub(super) cancel: CancellationToken,
    pub(super) jobs: Arc<Semaphore>,
    pub(super) notifications: Arc<Semaphore>,
    pub(super) tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
    pub(super) operations: tokio::sync::Mutex<()>,
}
impl Service {
    pub(super) fn new(
        provider: provider::Provider,
        directory: PathBuf,
        history: PathBuf,
        cancel: CancellationToken,
    ) -> Result<Arc<Self>> {
        let mut groups = vec![];
        if let Ok(mut file) = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(&history)
        {
            let metadata = file.metadata().map_err(|_| "history-unavailable")?;
            if !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
            {
                return Err("unsafe-history");
            }
            let mut bytes = vec![];
            (&mut file)
                .take(MAX_HISTORY as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "history-unavailable")?;
            if bytes.len() > MAX_HISTORY {
                return Err("history-too-large");
            }
            groups = serde_json::from_slice::<Vec<Group>>(&bytes).map_err(|_| "invalid-history")?;
            if groups.len() > MAX_GROUPS {
                return Err("history-too-large");
            }
            for g in &mut groups {
                if uuid::Uuid::parse_str(&g.id).is_err()
                    || !matches!(g.direction.as_str(), "incoming" | "outgoing")
                    || g.files.len() > 256
                    || !g.updated.is_finite()
                    || !g.created.is_finite()
                    || g.device.len() > 1024
                {
                    return Err("invalid-history");
                }
                for f in &g.files {
                    files::filename(&f.name)?;
                    if f.path.len() > 32768
                        || f.remote
                            .as_ref()
                            .is_some_and(|n| files::filename(n).is_err())
                    {
                        return Err("invalid-history");
                    }
                }
                if active(&g.state) {
                    g.state = "failed".into();
                    g.error = "interrupted".into();
                }
            }
        } else if history.symlink_metadata().is_ok() {
            return Err("unsafe-history");
        }
        let service = Arc::new(Self {
            state: Mutex::new(State {
                saved: Vec::new(),
                groups,
                controls: HashMap::new(),
                selection: vec![],
                targets: vec![],
                focus: String::new(),
                focus_revision: 0,
                error: String::new(),
            }),
            provider,
            directory,
            history,
            cancel,
            jobs: Arc::new(Semaphore::new(8)),
            notifications: Arc::new(Semaphore::new(16)),
            tasks: Mutex::new(vec![]),
            operations: tokio::sync::Mutex::new(()),
        });
        service.save(&mut service.state.lock().unwrap())?;
        Ok(service)
    }
    pub(super) fn save(&self, state: &mut State) -> Result<()> {
        state.prune();
        let data = serde_json::to_vec(&state.groups).map_err(|_| "history-unavailable")?;
        if data.len() > MAX_HISTORY {
            return Err("history-full");
        }
        if state.saved == data {
            return Ok(());
        }
        seele_runtime::fs::atomic_write(&self.history, &data).map_err(|_| "history-unavailable")?;
        state.saved = data;
        Ok(())
    }
    pub(super) fn snapshot(&self) -> Value {
        let mut state = self.state.lock().unwrap();
        state.prune();
        let groups: Vec<Value> = state
            .groups
            .iter()
            .map(|g| {
                let mut value = serde_json::to_value(g).unwrap();
                for f in value["files"].as_array_mut().unwrap() {
                    let f = f.as_object_mut().unwrap();
                    f.remove("remote");
                    f.remove("pendingAck");
                    f.remove("identity");
                    if g.direction == "outgoing" {
                        f.remove("path");
                    }
                }
                value["size"] = json!(g
                    .files
                    .iter()
                    .map(|f| f.size.max(0) as u64)
                    .fold(0u64, u64::saturating_add));
                value["bytes"] = json!(g
                    .files
                    .iter()
                    .map(|f| f.bytes)
                    .fold(0u64, u64::saturating_add));
                value
            })
            .collect();
        json!({"version":1,"groups":groups,"targets":state.targets,"selection":state.selection.iter().filter_map(|p|p.file_name().map(|n|n.to_string_lossy().into_owned())).collect::<Vec<_>>(),"focus":state.focus,"focusRevision":state.focus_revision,"capabilities":{"send":true,"receive":true,"resume":"provider-managed","cancelSend":true,"cancelRemoteReceive":false,"incomingSourceIdentity":false},"error":state.error})
    }
    pub(super) fn spawn(&self, task: impl std::future::Future<Output = ()> + Send + 'static) {
        let mut tasks = self.tasks.lock().unwrap();
        tasks.retain(|t| !t.is_finished());
        tasks.push(tokio::spawn(task));
    }
    pub(super) async fn dispatch(self: &Arc<Self>, request: Value) -> Result<Value> {
        if !request.is_object() {
            return Err("invalid-request");
        }
        let op = request["op"].as_str().ok_or("invalid-request")?;
        if op == "snapshot" {
            return Ok(self.snapshot());
        }
        let _guard = self.operations.lock().await;
        if op == "select" {
            let paths = request["paths"]
                .as_array()
                .filter(|p| p.len() <= 256)
                .ok_or("invalid-selection")?;
            let mut seen = HashSet::new();
            let mut selection = vec![];
            for p in paths {
                let path = files::regular(Path::new(
                    p.as_str()
                        .filter(|s| s.len() <= 32768)
                        .ok_or("invalid-selection")?,
                ))?;
                if seen.insert(path.clone()) {
                    selection.push(path);
                }
            }
            self.state.lock().unwrap().selection = selection;
            return Ok(json!({"ok":true}));
        }
        if op == "send" {
            if self.state.lock().unwrap().selection.is_empty() {
                return Err("choose-files");
            }
            let targets = self.provider.targets().await?;
            let target = targets
                .iter()
                .find(|t| t["id"] == request["target"])
                .ok_or("target-unavailable")?;
            let mut state = self.state.lock().unwrap();
            let mut entries = vec![];
            for path in &state.selection {
                let path = files::regular(path)?;
                let file = files::open_source(&path)?;
                let size = file.metadata().map_err(|_| "source-missing")?.len();
                if size > i64::MAX as u64 {
                    return Err("source-too-large");
                }
                entries.push(Entry {
                    identity: Some(files::Identity::of(
                        &file.metadata().map_err(|_| "source-missing")?,
                    )),
                    name: path.file_name().unwrap().to_string_lossy().into_owned(),
                    path: path.to_string_lossy().into_owned(),
                    remote: None,
                    pending_ack: false,
                    size: size as i64,
                    bytes: 0,
                    state: "pending".into(),
                    error: String::new(),
                    attempts: 0,
                });
            }
            if self.jobs.available_permits() == 0 {
                return Err("too-many-transfers");
            }
            let id = state.new_group("outgoing", target["name"].as_str().unwrap(), entries)?;
            state.group(&id)?.target = Some(target["id"].as_str().unwrap().to_owned());
            if let Err(error) = self.start_locked(&mut state, &id) {
                state.groups.retain(|g| g.id != id);
                return Err(error);
            }
            state.selection.clear();
            return Ok(json!({"ok":true,"id":id}));
        }
        if op == "seen" {
            let ids: HashSet<&str> = if let Some(values) = request.get("ids") {
                values
                    .as_array()
                    .filter(|values| !values.is_empty() && values.len() <= MAX_GROUPS)
                    .ok_or("invalid-request")?
                    .iter()
                    .map(|id| {
                        id.as_str()
                            .filter(|id| !id.is_empty() && id.len() <= 128)
                            .ok_or("invalid-request")
                    })
                    .collect::<Result<_>>()?
            } else {
                [request["id"].as_str().ok_or("transfer-missing")?]
                    .into_iter()
                    .collect()
            };
            let mut state = self.state.lock().unwrap();
            let existing: HashSet<_> = state.groups.iter().map(|group| group.id.as_str()).collect();
            if !ids.is_subset(&existing) {
                return Err("transfer-missing");
            }
            let changed: HashSet<_> = state
                .groups
                .iter()
                .filter(|group| !group.seen && ids.contains(group.id.as_str()))
                .map(|group| group.id.clone())
                .collect();
            for group in &mut state.groups {
                if changed.contains(&group.id) {
                    group.seen = true;
                }
            }
            if let Err(error) = self.save(&mut state) {
                for group in &mut state.groups {
                    if changed.contains(&group.id) {
                        group.seen = false;
                    }
                }
                return Err(error);
            }
            return Ok(json!({"ok":true}));
        }
        let id = request["id"].as_str().ok_or("transfer-missing")?.to_owned();
        if matches!(op, "open" | "reveal" | "trash" | "move") {
            self.file_action(&id, &request).await?;
            return Ok(json!({"ok":true}));
        }
        let mut state = self.state.lock().unwrap();
        match op {
            "cancel" => state.controls.get(&id).ok_or("cancel-at-sender")?.cancel(),
            "retry" => {
                if !matches!(state.group(&id)?.state.as_str(), "failed" | "cancelled") {
                    return Err("already-active");
                }
                self.start_locked(&mut state, &id)?;
            }
            "dismiss" => {
                if active(&state.group(&id)?.state) {
                    return Err("already-active");
                }
                state.groups.retain(|g| g.id != id);
            }
            "focus" => {
                state.group(&id)?;
                state.focus = id;
                state.focus_revision = state.focus_revision.wrapping_add(1);
            }
            _ => return Err("unknown-operation"),
        }
        self.save(&mut state)?;
        Ok(json!({"ok":true}))
    }
    pub(super) async fn file_action(&self, id: &str, request: &Value) -> Result<()> {
        let index = request["file"].as_u64().ok_or("file-missing")? as usize;
        let path = {
            let mut state = self.state.lock().unwrap();
            let group = state.group(id)?;
            if group.direction != "incoming" {
                return Err("not-incoming");
            }
            let entry = group.files.get(index).ok_or("file-missing")?;
            if entry.state != "completed" {
                return Err("file-unavailable");
            }
            files::regular(Path::new(&entry.path))?
        };
        let op = request["op"].as_str().unwrap();
        if op == "move" {
            let directory = PathBuf::from(
                request["directory"]
                    .as_str()
                    .ok_or("destination-unavailable")?,
            );
            let next = tokio::task::spawn_blocking(move || files::move_file(&path, &directory))
                .await
                .map_err(|_| "desktop-action-failed")??;
            let mut state = self.state.lock().unwrap();
            state.group(id)?.files[index].path = next.to_string_lossy().into_owned();
            self.save(&mut state)?;
        } else {
            let mut command = if op == "trash" {
                let mut c = Command::new("gio");
                c.args(["trash", "--"]).arg(&path);
                c
            } else {
                let mut c = Command::new("xdg-open");
                c.arg(if op == "open" {
                    path.as_path()
                } else {
                    path.parent().ok_or("file-missing")?
                });
                c
            };
            command.env_remove("LD_PRELOAD");
            let result = if op == "trash" {
                common::command(
                    command,
                    vec![],
                    Duration::from_secs(15),
                    65536,
                    self.cancel.clone(),
                )
                .await
                .map(|_| ())
            } else {
                common::launch(command, Duration::from_secs(15), self.cancel.clone()).await
            };
            result.map_err(|_| "desktop-action-failed")?;
            if op == "trash" {
                let mut state = self.state.lock().unwrap();
                let entry = &mut state.group(id)?.files[index];
                entry.state = "trashed".into();
                entry.path.clear();
                self.save(&mut state)?;
            }
        }
        Ok(())
    }
    pub(super) fn start_locked(self: &Arc<Self>, state: &mut State, id: &str) -> Result<()> {
        if state.controls.contains_key(id) {
            return Err("already-active");
        }
        let permit = self
            .jobs
            .clone()
            .try_acquire_owned()
            .map_err(|_| "too-many-transfers")?;
        let group = state.group(id)?;
        group.state = if group.direction == "outgoing" {
            "sending"
        } else {
            "receiving"
        }
        .into();
        group.error.clear();
        group.updated = now();
        self.save(state)?;
        let cancel = self.cancel.child_token();
        state.controls.insert(id.to_owned(), cancel.clone());
        let service = self.clone();
        let id = id.to_owned();
        self.spawn(async move {
            let _permit = permit;
            service.run(id, cancel).await;
        });
        Ok(())
    }
    pub(super) fn progress(&self, id: &str, index: usize, count: u64, size: Option<u64>) {
        let mut state = self.state.lock().unwrap();
        if let Ok(group) = state.group(id) {
            if let Some(entry) = group.files.get_mut(index) {
                entry.bytes = if let Some(size) = size {
                    entry.size = size.min(i64::MAX as u64) as i64;
                    count
                } else {
                    count.min(entry.size.max(0) as u64)
                };
                group.updated = now();
            }
        }
    }
    pub(super) async fn run(self: Arc<Self>, id: String, cancel: CancellationToken) {
        let result = tokio::select! { result = self.run_files(&id, &cancel) => result, _ = cancel.cancelled() => Err("cancelled") };
        let notification = {
            let mut state = self.state.lock().unwrap();
            let group = match state.group(&id) {
                Ok(g) => g,
                Err(_) => return,
            };
            match result {
                Ok(()) => {
                    group.state = "completed".into();
                    group.error.clear();
                }
                Err(error) => {
                    let cancelled = cancel.is_cancelled() || error == "cancelled";
                    group.state = if cancelled { "cancelled" } else { "failed" }.into();
                    group.error = if cancelled { "cancelled" } else { error }.into();
                }
            }
            group.updated = now();
            let notification = if group.state == "failed"
                || (group.state == "completed" && group.direction == "incoming")
            {
                Some(group.clone())
            } else {
                None
            };
            if let Err(error) = self.save(&mut state) {
                state.error = error.into();
            }
            state.controls.remove(&id);
            notification
        };
        if let Some(group) = notification {
            self.notify(group);
        }
    }
    pub(super) async fn run_files(
        self: &Arc<Self>,
        id: &str,
        cancel: &CancellationToken,
    ) -> Result<()> {
        let count = self.state.lock().unwrap().group(id)?.files.len();
        for index in 0..count {
            let group = self.state.lock().unwrap().group(id)?.clone();
            let entry = &group.files[index];
            if entry.pending_ack {
                self.provider
                    .forget(entry.remote.as_deref().ok_or("invalid-history")?)
                    .await?;
                let mut state = self.state.lock().unwrap();
                let entry = &mut state.group(id)?.files[index];
                entry.pending_ack = false;
                entry.remote = None;
                entry.error.clear();
                self.save(&mut state)?;
                continue;
            }
            if matches!(entry.state.as_str(), "completed" | "trashed") {
                continue;
            }
            for attempt in 0..3 {
                if cancel.is_cancelled() {
                    return Err("cancelled");
                }
                {
                    let mut state = self.state.lock().unwrap();
                    let group = state.group(id)?;
                    group.state = if group.direction == "outgoing" {
                        "sending"
                    } else {
                        "receiving"
                    }
                    .into();
                    let entry = &mut group.files[index];
                    entry.state = "active".into();
                    entry.error.clear();
                    entry.bytes = 0;
                    entry.attempts = entry.attempts.saturating_add(1);
                }
                let result = async {
                    if group.direction == "outgoing" {
                        let target = group.target.as_deref().ok_or("target-unavailable")?;
                        if !self
                            .provider
                            .targets()
                            .await?
                            .iter()
                            .any(|t| t["id"] == target)
                        {
                            return Err("target-unavailable");
                        }
                        let path = files::regular(Path::new(&entry.path))?;
                        self.provider
                            .send(
                                target,
                                &path,
                                entry.size.max(0) as u64,
                                entry.identity.as_ref(),
                                cancel,
                            )
                            .await?;
                    } else {
                        let name = entry.remote.as_deref().ok_or("receive-unavailable")?;
                        let (path, size) = self
                            .provider
                            .receive(name, &self.directory, cancel, self, id, index)
                            .await?;
                        // Publish and durably journal first. Daemon deletion follows only a successful barrier.
                        {
                            let mut state = self.state.lock().unwrap();
                            let entry = &mut state.group(id)?.files[index];
                            entry.path = path;
                            entry.size = size.min(i64::MAX as u64) as i64;
                            entry.bytes = size;
                            entry.state = "completed".into();
                            entry.pending_ack = true;
                            self.save(&mut state)?;
                        }
                        self.provider.forget(name).await?;
                        let mut state = self.state.lock().unwrap();
                        let entry = &mut state.group(id)?.files[index];
                        entry.pending_ack = false;
                        entry.remote = None;
                    }
                    let mut state = self.state.lock().unwrap();
                    let entry = &mut state.group(id)?.files[index];
                    entry.state = "completed".into();
                    entry.bytes = entry.size.max(0) as u64;
                    entry.error.clear();
                    self.save(&mut state)?;
                    Ok(())
                }
                .await;
                match result {
                    Ok(()) => break,
                    Err(error) => {
                        if cancel.is_cancelled() {
                            return Err("cancelled");
                        }
                        let pending = {
                            let mut state = self.state.lock().unwrap();
                            let group = state.group(id)?;
                            let entry = &mut group.files[index];
                            if !entry.pending_ack {
                                entry.error = error.into();
                                entry.state = "failed".into();
                            }
                            let pending = entry.pending_ack;
                            group.state = "retrying".into();
                            pending
                        };
                        if pending
                            || attempt == 2
                            || matches!(
                                error,
                                "source-missing"
                                    | "source-changed"
                                    | "target-unavailable"
                                    | "not-a-file"
                                    | "history-unavailable"
                                    | "history-full"
                            )
                        {
                            return Err(error);
                        }
                        tokio::select! {_=tokio::time::sleep(Duration::from_secs(1<<attempt))=>{},_=cancel.cancelled()=>return Err("cancelled")};
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) async fn receive_waiting(self: &Arc<Self>) -> Result<()> {
        let waiting = self.provider.waiting().await?;
        for remote in waiting {
            let name = remote["name"].as_str().ok_or("invalid-response")?;
            let mut state = self.state.lock().unwrap();
            let existing = state
                .groups
                .iter()
                .find(|g| {
                    g.direction == "incoming"
                        && g.files.iter().any(|f| f.remote.as_deref() == Some(name))
                })
                .map(|g| g.id.clone());
            if let Some(id) = existing {
                if state.controls.contains_key(&id) {
                    continue;
                }
                let group = state.group(&id)?;
                if group.files.iter().any(|e| e.pending_ack) || group.state == "receiving" {
                    let _ = self.start_locked(&mut state, &id);
                }
                continue;
            }
            if self.jobs.available_permits() == 0 {
                break;
            }
            let entry = Entry {
                identity: None,
                name: name.into(),
                path: String::new(),
                remote: Some(name.into()),
                pending_ack: false,
                size: remote["size"].as_i64().ok_or("invalid-response")?,
                bytes: 0,
                state: "pending".into(),
                error: String::new(),
                attempts: 0,
            };
            let id = state.new_group("incoming", "Personal device", vec![entry])?;
            self.start_locked(&mut state, &id)?;
        }
        Ok(())
    }
    pub(super) fn event(&self, message: &Value) {
        let mut state = self.state.lock().unwrap();
        for outgoing in message["OutgoingFiles"]
            .as_array()
            .into_iter()
            .flatten()
            .take(4096)
        {
            if outgoing["Finished"] == true {
                continue;
            }
            let bytes = outgoing["Sent"].as_u64().unwrap_or(0);
            for group in &mut state.groups {
                if group.state == "sending"
                    && group.target.as_deref() == outgoing["PeerID"].as_str()
                {
                    for entry in &mut group.files {
                        if entry.state == "active"
                            && Some(entry.name.as_str()) == outgoing["Name"].as_str()
                        {
                            entry.bytes = bytes.min(entry.size.max(0) as u64);
                            group.updated = now();
                        }
                    }
                }
            }
        }
        for incoming in message["IncomingFiles"]
            .as_array()
            .into_iter()
            .flatten()
            .take(4096)
        {
            let Some(name) = incoming["Name"]
                .as_str()
                .filter(|n| files::filename(n).is_ok())
            else {
                continue;
            };
            let existing = state
                .groups
                .iter()
                .find(|g| {
                    g.direction == "incoming"
                        && g.files
                            .first()
                            .is_some_and(|e| e.remote.as_deref() == Some(name))
                })
                .map(|g| (g.id.clone(), g.state.clone()));
            if existing
                .as_ref()
                .is_some_and(|(_, s)| matches!(s.as_str(), "failed" | "cancelled"))
            {
                continue;
            }
            let size = incoming["DeclaredSize"].as_i64().unwrap_or(-1);
            let bytes = incoming["Received"].as_u64().unwrap_or(0);
            let id = if let Some((id, _)) = existing {
                id
            } else {
                let entry = Entry {
                    identity: None,
                    name: name.into(),
                    path: String::new(),
                    remote: Some(name.into()),
                    pending_ack: false,
                    size,
                    bytes: 0,
                    state: "active".into(),
                    error: String::new(),
                    attempts: 0,
                };
                match state.new_group("incoming", "Personal device", vec![entry]) {
                    Ok(id) => id,
                    Err(_) => continue,
                }
            };
            if !state.controls.contains_key(&id) {
                if let Ok(group) = state.group(&id) {
                    group.files[0].bytes = bytes;
                    group.files[0].size = size;
                    group.updated = now();
                }
            }
        }
    }
    pub(super) async fn poll(self: Arc<Self>) {
        if std::fs::create_dir_all(&self.directory).is_err() {
            self.state.lock().unwrap().error = "destination-unavailable".into();
        }
        loop {
            let work = async {
                let targets = self.provider.targets().await?;
                {
                    let mut state = self.state.lock().unwrap();
                    state.targets = targets;
                    state.error.clear();
                }
                self.receive_waiting().await?;
                let mut notifications = vec![];
                {
                    let mut state = self.state.lock().unwrap();
                    let controls: HashSet<_> = state.controls.keys().cloned().collect();
                    for group in &mut state.groups {
                        if group.state == "receiving"
                            && !controls.contains(&group.id)
                            && group.updated < now() - 90.
                        {
                            group.state = "failed".into();
                            group.error = "interrupted".into();
                            group.updated = now();
                            notifications.push(group.clone());
                        }
                    }
                    self.save(&mut state)?;
                }
                for group in notifications {
                    self.notify(group);
                }
                Ok::<_, &'static str>(())
            };
            tokio::select! {result=work=>if let Err(error)=result{let mut state=self.state.lock().unwrap();state.targets.clear();state.error=error.into();},_=self.cancel.cancelled()=>break};
            tokio::select! {_=tokio::time::sleep(Duration::from_secs(2))=>{},_=self.cancel.cancelled()=>break};
        }
    }
    pub(super) fn notify(self: &Arc<Self>, group: Group) {
        let Ok(permit) = self.notifications.clone().try_acquire_owned() else {
            return;
        };
        let service = self.clone();
        self.spawn(async move {
            let _permit = permit;
            let mut command = Command::new("notify-send");
            command
                .args([
                    "--app-name=Seele Transfers",
                    "--icon=folder-download",
                    "--wait",
                    "--action=open=Open Transfers",
                    if group.state == "failed" {
                        "Transfer failed"
                    } else {
                        "File received"
                    },
                ])
                .arg(format!(
                    "{} file(s) · {}",
                    group.files.len(),
                    group
                        .device
                        .replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                ));
            if let Ok(output) = common::command(
                command,
                vec![],
                Duration::from_secs(86400),
                4096,
                service.cancel.clone(),
            )
            .await
            {
                if output == b"open\n" || output == b"open" {
                    let _ = service.dispatch(json!({"op":"focus","id":group.id})).await;
                    let _ = open_panel(service.cancel.clone()).await;
                }
            }
        });
    }
    pub(super) async fn shutdown(&self) {
        self.cancel.cancel();
        let tasks = std::mem::take(&mut *self.tasks.lock().unwrap());
        for task in tasks {
            let _ = task.await;
        }
    }
}
