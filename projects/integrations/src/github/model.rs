use super::api::{self, text};
use crate::common::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const PRIORITIES: [&str; 4] = [
    "Immediate Action required",
    "Action required soon",
    "Action required sometime",
    "Informational",
];
pub fn digest(value: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap_or_default())
    )
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub repository: String,
    pub reason: String,
    pub kind: String,
    pub updated_at: String,
    pub unread: bool,
    pub subject: String,
    pub url: String,
    pub revision: String,
    pub repository_description: String,
}
impl Thread {
    pub fn parse(v: &Value, host: &str) -> Result<Self> {
        let id = text(&v["id"]);
        if !api::numeric(&id) {
            return Err("invalid-response");
        }
        let repository = text(&v["repository"]["full_name"]);
        if repository.split('/').count() != 2 || !repository.split('/').all(api::component) {
            return Err("invalid-response");
        }
        let subject = text(&v["subject"]["url"]);
        let url = api::subject_path(&subject, host, &repository)
            .and_then(|path| {
                let parts: Vec<_> = path.split('/').collect();
                let kind = match (text(&v["subject"]["type"]).as_str(), parts[3]) {
                    ("Issue", "issues") => "issues",
                    ("PullRequest", "pulls") => "pull",
                    ("Discussion", "discussions") => "discussions",
                    ("Commit", "commits") => "commit",
                    _ => return None,
                };
                Some(format!("https://{host}/{repository}/{kind}/{}", parts[4]))
            })
            .unwrap_or_default();
        Ok(Self {
            id,
            title: text(&v["subject"]["title"]),
            repository,
            reason: text(&v["reason"]),
            kind: text(&v["subject"]["type"]),
            updated_at: text(&v["updated_at"]),
            unread: v["unread"] == true,
            subject,
            url,
            revision: digest(&json!([v["updated_at"], v["subject"], v["reason"]])),
            repository_description: text(&v["repository"]["description"]),
        })
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub body: String,
    pub author: String,
    pub created_at: String,
    pub updated_at: String,
    pub state: String,
    pub labels: Vec<String>,
    pub url: String,
    pub comments: Vec<Value>,
    pub reviews: Vec<Value>,
    pub checks: Value,
    pub unavailable: String,
}
impl Detail {
    pub fn unsupported(t: &Thread) -> Self {
        Self{updated_at:t.updated_at.clone(),unavailable:format!("GitHub does not expose the complete {} thread through this integration. The original notification remains in the inbox.",t.kind),checks:json!([]),..Default::default()}
    }
    pub fn subject(v: &Value, t: &Thread, host: &str) -> Self {
        Self {
            body: text(&v["body"]),
            author: text(&v["user"]["login"]).or_else_string(|| text(&v["author"]["login"])),
            created_at: text(&v["created_at"]),
            updated_at: text(&v["updated_at"]),
            state: if v["merged"] == true {
                "merged".into()
            } else {
                text(&v["state"])
            },
            labels: v["labels"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x["name"].as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default(),
            url: api::safe_web(&v["html_url"], host).unwrap_or_default(),
            checks: json!([]),
            ..Self::unsupported(t).available()
        }
    }
    fn available(mut self) -> Self {
        self.unavailable.clear();
        self
    }
}
trait StringFallback {
    fn or_else_string(self, f: impl FnOnce() -> String) -> String;
}
impl StringFallback for String {
    fn or_else_string(self, f: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            f()
        } else {
            self
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Triage {
    pub summary: String,
    pub reason: String,
    pub attention: String,
    pub next_action: String,
    pub priority: String,
    pub changes: String,
}
impl Triage {
    pub fn parse(v: Value) -> Result<Self> {
        let result: Self = serde_json::from_value(v).map_err(|_| "invalid-output")?;
        if !PRIORITIES.contains(&result.priority.as_str())
            || [
                &result.summary,
                &result.reason,
                &result.attention,
                &result.next_action,
                &result.changes,
            ]
            .iter()
            .any(|s| s.trim().is_empty() || s.len() > 4096)
        {
            return Err("invalid-output");
        }
        Ok(result)
    }
    pub fn rank(&self) -> usize {
        PRIORITIES
            .iter()
            .position(|p| *p == self.priority)
            .unwrap_or(4)
    }
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub thread: Thread,
    pub detail: Option<Detail>,
    pub analyzed: Option<SourceStamp>,
    pub triage: Option<Triage>,
    pub previous: Option<Triage>,
    pub state: &'static str,
    pub error: &'static str,
    pub token: u64,
    pub pending_done: bool,
    pub retry_at: std::time::Instant,
    pub attempts: u8,
    pub checked_at: std::time::Instant,
}
impl Entry {
    pub fn new(thread: Thread, token: u64) -> Self {
        Self {
            thread,
            detail: None,
            analyzed: None,
            triage: None,
            previous: None,
            state: "pending",
            error: "",
            token,
            pending_done: false,
            retry_at: std::time::Instant::now(),
            attempts: 0,
            checked_at: std::time::Instant::now(),
        }
    }
    pub fn row(&self) -> Value {
        json!({"id":self.thread.id,"title":self.thread.title,"repository":self.thread.repository,"reason":self.thread.reason,"kind":self.thread.kind,"updatedAt":self.thread.updated_at,"unread":self.thread.unread,"state":self.state,"error":message(self.error),"triage":self.triage.as_ref().map(|t|json!({"priority":t.priority,"summary":t.summary})),"pendingDone":self.pending_done})
    }
    pub fn view(&self) -> Value {
        json!({"thread":self.thread,"detail":self.detail,"triage":self.triage,"previous":self.previous,"state":self.state,"error":message(self.error),"pendingDone":self.pending_done,"blocks":self.blocks()})
    }
    fn blocks(&self) -> Vec<Value> {
        let mut blocks = vec![
            json!({"label":self.thread.title,"body":format!("{} · {} · {}\nUpdated {}",self.thread.repository,self.thread.kind,self.thread.reason,self.thread.updated_at)}),
        ];
        if self.state != "ready" {
            blocks.push(json!({"label":if self.state=="failed"{"Triage failed"}else{"Triage pending"},"body":if self.error.is_empty(){"The original thread remains available below."}else{message(self.error)}}));
        }
        if let Some(t) = &self.triage {
            for (label, body) in [
                (t.priority.as_str(), &t.summary),
                ("Why you were notified", &t.reason),
                ("Needs your attention", &t.attention),
                ("Suggested next action", &t.next_action),
                ("What changed", &t.changes),
            ] {
                blocks.push(json!({"label":label,"body":body}));
            }
        }
        if let Some(t) = &self.previous {
            blocks.push(json!({"label":"Previous triage","meta":t.priority,"body":t.summary}));
        }
        if let Some(d) = &self.detail {
            blocks.push(json!({"label":"Thread","meta":format!("{} · {}\nCreated {} · Updated {}\n{}",d.author,d.state,d.created_at,d.updated_at,d.labels.join(" · ")),"body":d.body}));
            if !d.unavailable.is_empty() {
                blocks.push(json!({"label":"Source availability","body":d.unavailable}));
            }
            for c in &d.comments {
                blocks.push(json!({"label":"Comment","meta":format!("{} · {}",text(&c["author"]),text(&c["updatedAt"])),"body":text(&c["body"])}));
            }
            for r in &d.reviews {
                blocks.push(json!({"label":"Review","meta":format!("{} · {} · {}",text(&r["author"]),text(&r["state"]),text(&r["updatedAt"])),"body":text(&r["body"])}));
            }
            if let Some(checks) = d.checks.as_array() {
                for c in checks {
                    blocks.push(json!({"label":"CI","meta":text(&c["name"]).or_else_string(||text(&c["context"])),"body":format!("{} {} {}",text(&c["status"]).or_else_string(||text(&c["state"])),text(&c["conclusion"]),text(&c["description"]))}));
                }
            }
        } else {
            blocks.push(json!({"label":"Thread","body":"Loading the original thread…"}));
        }
        let mut keys = BTreeMap::<String, usize>::new();
        for block in &mut blocks {
            let label = text(&block["label"]);
            let number = keys.entry(label.clone()).or_default();
            block["key"] = json!(format!("{label}:{number}"));
            *number += 1;
        }
        blocks
    }
}
#[derive(Default, Serialize, Deserialize)]
pub struct Ledger {
    #[serde(default)]
    pub notified: BTreeMap<String, String>,
    #[serde(default)]
    pub done: BTreeMap<String, String>,
}
pub fn rows(entries: &BTreeMap<String, Entry>) -> Vec<Value> {
    let mut list: Vec<_> = entries.values().filter(|e| !e.pending_done).collect();
    list.sort_by(|a, b| {
        a.triage
            .as_ref()
            .map_or(4, Triage::rank)
            .cmp(&b.triage.as_ref().map_or(4, Triage::rank))
            .then_with(|| b.thread.updated_at.cmp(&a.thread.updated_at))
            .then_with(|| a.thread.id.cmp(&b.thread.id))
    });
    list.into_iter().map(Entry::row).collect()
}
#[derive(Clone, Debug)]
pub struct SourceStamp {
    pub fingerprint: String,
    body: String,
    state: String,
    labels: String,
    comments: String,
    reviews: String,
    checks: String,
    last_comment: String,
}
impl SourceStamp {
    pub fn new(detail: &Detail) -> Self {
        Self {
            fingerprint: digest(detail),
            body: digest(&detail.body),
            state: detail.state.clone(),
            labels: digest(&detail.labels),
            comments: digest(&detail.comments),
            reviews: digest(&detail.reviews),
            checks: digest(&detail.checks),
            last_comment: detail
                .comments
                .iter()
                .filter_map(|c| c["updatedAt"].as_str())
                .max()
                .unwrap_or_default()
                .into(),
        }
    }
}
pub fn changed(old: Option<&SourceStamp>, new: &Detail) -> Value {
    let Some(old) = old else { return Value::Null };
    json!({"bodyChanged":old.body!=digest(&new.body),"stateBefore":old.state,"stateAfter":new.state,"labelsChanged":old.labels!=digest(&new.labels),"commentsChanged":old.comments!=digest(&new.comments),"reviewsChanged":old.reviews!=digest(&new.reviews),"checksChanged":old.checks!=digest(&new.checks),"newOrEditedComments":new.comments.iter().filter(|c|c["updatedAt"].as_str().is_some_and(|time|time>old.last_comment.as_str())).collect::<Vec<_>>()})
}
pub fn message(code: &str) -> &'static str {
    match code {
    ""=>"", "auth-required"=>"Sign in with gh auth login, then refresh.", "access-denied"=>"GitHub denied access. Check the notifications/repo scopes and organization access for your gh login.",
    "rate-limited"=>"GitHub rate limit reached. Automatic requests pause for five minutes.", "cli-unavailable"=>"GitHub CLI is unavailable.", "invalid-host"=>"The configured GitHub host is invalid.",
    "context-too-large"|"response-too-large"=>"This response exceeds the safety limit. The raw notification is retained; open GitHub for the full thread.",
    "inbox-too-large"=>"Inbox refresh is incomplete: the safety limit was reached. Loaded notifications remain available.",
    "broker-unavailable"=>"AI triage is unavailable. The notification is ready to read; retry triage when the broker is available.",
    "invalid-output"=>"AI returned an invalid triage result. Retry triage.", "state-unavailable"=>"Private notification state could not be saved. Refresh to reconcile with GitHub.",
    "context-unavailable"|"not-found"=>"GitHub could not provide this thread. Check repository access or open GitHub.",
    "write-failed"=>"GitHub did not confirm Done. The notification has been restored; refresh or retry.",
    _=>"GitHub could not be reached. The last confirmed inbox is retained; refresh to retry.",
}
}
