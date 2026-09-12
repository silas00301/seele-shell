//! Persistent metadata and its provider-neutral lifecycle schema.
use super::{files, MAX_GROUPS};
use crate::common::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;
pub(super) fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
pub(super) fn active(state: &str) -> bool {
    matches!(state, "sending" | "receiving" | "retrying")
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Entry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) identity: Option<files::Identity>,
    pub(super) name: String,
    #[serde(default)]
    pub(super) path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) remote: Option<String>,
    #[serde(
        default,
        rename = "pendingAck",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub(super) pending_ack: bool,
    pub(super) size: i64,
    pub(super) bytes: u64,
    pub(super) state: String,
    #[serde(default)]
    pub(super) error: String,
    #[serde(default)]
    pub(super) attempts: u64,
}
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Group {
    pub(super) id: String,
    pub(super) direction: String,
    pub(super) device: String,
    pub(super) files: Vec<Entry>,
    pub(super) state: String,
    pub(super) created: f64,
    pub(super) updated: f64,
    pub(super) error: String,
    pub(super) seen: bool,
    pub(super) source: String,
    pub(super) destination: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) target: Option<String>,
}
pub(super) struct State {
    pub(super) saved: Vec<u8>,
    pub(super) groups: Vec<Group>,
    pub(super) controls: HashMap<String, CancellationToken>,
    pub(super) selection: Vec<PathBuf>,
    pub(super) targets: Vec<Value>,
    pub(super) focus: String,
    pub(super) focus_revision: u64,
    pub(super) error: String,
}
impl State {
    pub(super) fn group(&mut self, id: &str) -> Result<&mut Group> {
        self.groups
            .iter_mut()
            .find(|g| g.id == id)
            .ok_or("transfer-missing")
    }
    pub(super) fn prune(&mut self) {
        let cutoff = now() - 7. * 86400.;
        self.groups
            .retain(|g| active(&g.state) || g.state == "failed" || g.updated > cutoff);
    }
    pub(super) fn new_group(
        &mut self,
        direction: &str,
        device: &str,
        files: Vec<Entry>,
    ) -> Result<String> {
        self.prune();
        if self.groups.len() >= MAX_GROUPS {
            return Err("history-full");
        }
        let id = uuid::Uuid::new_v4().to_string();
        let time = now();
        self.groups.insert(
            0,
            Group {
                id: id.clone(),
                direction: direction.into(),
                device: device.into(),
                files,
                state: if direction == "outgoing" {
                    "sending"
                } else {
                    "receiving"
                }
                .into(),
                created: time,
                updated: time,
                error: String::new(),
                seen: direction == "outgoing",
                source: if direction == "incoming" {
                    device
                } else {
                    "This device"
                }
                .into(),
                destination: if direction == "incoming" {
                    "This device"
                } else {
                    device
                }
                .into(),
                target: None,
            },
        );
        Ok(id)
    }
}
