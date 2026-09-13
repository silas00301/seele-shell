//! Only gh owns credentials. Every endpoint is constructed here; subject URLs
//! are validated identifiers, never arbitrary URLs forwarded to an authenticated CLI.
use super::model::{Detail, Thread};
use crate::common::Result;
use serde_json::{json, Value};
use std::{
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Api {
    pub host: String,
    pub cancel: CancellationToken,
}
impl Api {
    pub fn new(cancel: CancellationToken) -> Result<Self> {
        let host = std::env::var("SEELE_GITHUB_HOST")
            .unwrap_or_else(|_| "github.com".into())
            .to_lowercase();
        if host.len() > 253
            || host.split('.').any(|p| {
                p.is_empty()
                    || p.starts_with('-')
                    || p.ends_with('-')
                    || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err("invalid-host");
        }
        Ok(Self { host, cancel })
    }
    pub async fn request(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value> {
        let mut command = Command::new("gh");
        command
            .args([
                "api",
                "--hostname",
                &self.host,
                "--method",
                method,
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                "X-GitHub-Api-Version: 2026-03-10",
                path,
            ])
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat")
            .env("NO_COLOR", "1")
            .env_remove("GH_DEBUG");
        let input = if let Some(body) = body {
            command.args(["--input", "-"]);
            serde_json::to_vec(&body).map_err(|_| "invalid-response")?
        } else {
            vec![]
        };
        let flag = Arc::new(AtomicUsize::new(0));
        struct Guard(Arc<AtomicUsize>);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.store(1, Ordering::Relaxed);
            }
        }
        let _guard = Guard(flag.clone());
        let mut task = tokio::task::spawn_blocking(move || {
            seele_runtime::process::capture(
                &mut command,
                &input,
                seele_runtime::process::Limits {
                    timeout: Duration::from_secs(20),
                    output: 4 * 1024 * 1024,
                },
                &flag,
            )
        });
        let output = tokio::select! { value = &mut task => value, _ = self.cancel.cancelled() => return Err("cancelled") }
            .map_err(|_| "unavailable")?.map_err(|e| match e.kind() { std::io::ErrorKind::NotFound => "cli-unavailable", std::io::ErrorKind::InvalidData => "response-too-large", _ => "unavailable" })?;
        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr).to_lowercase();
            return Err(
                if error.contains("rate limit") || error.contains("http 429") {
                    "rate-limited"
                } else if error.contains("http 401")
                    || error.contains("bad credentials")
                    || error.contains("gh auth login")
                    || error.contains("not logged")
                {
                    "auth-required"
                } else if error.contains("http 403") {
                    "access-denied"
                } else if error.contains("http 404") {
                    "not-found"
                } else {
                    "unavailable"
                },
            );
        }
        if output.stdout.is_empty() {
            return Ok(Value::Null);
        }
        let value: Value =
            serde_json::from_slice(&output.stdout).map_err(|_| "invalid-response")?;
        if value["errors"]
            .as_array()
            .is_some_and(|errors| !errors.is_empty())
        {
            return Err("context-unavailable");
        }
        Ok(value)
    }
    pub async fn get(&self, path: &str) -> Result<Value> {
        self.request("GET", path, None).await
    }
    pub async fn graphql(&self, query: &str, variables: Value) -> Result<Value> {
        self.request(
            "POST",
            "graphql",
            Some(json!({"query":query,"variables":variables})),
        )
        .await
    }
    pub async fn identity(&self) -> Result<(String, String)> {
        let user = self.get("user").await?;
        let id = user["id"]
            .as_u64()
            .filter(|id| *id > 0)
            .ok_or("invalid-response")?;
        let login = user["login"]
            .as_str()
            .filter(|s| component(s))
            .ok_or("invalid-response")?;
        Ok((format!("{}:{id}", self.host), login.into()))
    }
    pub async fn page(&self, page: usize) -> Result<Vec<Thread>> {
        let value = self
            .get(&format!(
                "notifications?all=false&participating=false&per_page=50&page={page}"
            ))
            .await?;
        value
            .as_array()
            .ok_or("invalid-response")?
            .iter()
            .map(|v| Thread::parse(v, &self.host))
            .collect()
    }
    async fn pages(&self, path: &str) -> Result<Vec<Value>> {
        let mut items = Vec::new();
        for page in 1..=200 {
            let value = self.get(&format!("{path}?per_page=50&page={page}")).await?;
            let batch = value.as_array().ok_or("invalid-response")?;
            items.extend(batch.iter().cloned());
            bounded(&items)?;
            if batch.len() < 50 {
                return Ok(items);
            }
        }
        Err("context-too-large")
    }
    pub async fn done(&self, id: &str) -> Result<()> {
        if !numeric(id) {
            return Err("invalid-request");
        }
        self.request("DELETE", &format!("notifications/threads/{id}"), None)
            .await
            .map(|_| ())
    }
    pub async fn detail(&self, thread: &Thread) -> Result<Detail> {
        let Some(path) = subject_path(&thread.subject, &self.host, &thread.repository) else {
            return Ok(Detail::unsupported(thread));
        };
        let parts: Vec<_> = path.split('/').collect();
        let kind = parts[3];
        let id = parts[4];
        let repo = &thread.repository;
        let mut detail = Detail::unsupported(thread);
        match (thread.kind.as_str(), kind) {
            ("Issue", "issues") | ("PullRequest", "pulls") if numeric(id) => {
                let subject = self.get(&path).await?;
                detail = Detail::subject(&subject, thread, &self.host);
                detail.comments = self
                    .pages(&format!("repos/{repo}/issues/{id}/comments"))
                    .await?
                    .iter()
                    .map(comment)
                    .collect();
                if kind == "pulls" {
                    // Review comment REST payloads contain diff_hunk. Use explicit
                    // GraphQL fields instead, so diffs never enter this process.
                    let reviews = self
                        .pages(&format!("repos/{repo}/pulls/{id}/reviews"))
                        .await?;
                    for review in reviews {
                        detail.reviews.push(json!({"author":review["user"]["login"],"state":review["state"],"body":review["body"],"updatedAt":review["submitted_at"]}));
                        if let Some(node) = review["node_id"].as_str() {
                            detail.comments.extend(
                                self.node_comments(node, "PullRequestReview", "comments")
                                    .await?,
                            );
                        }
                        bounded(&detail)?;
                    }
                    detail.checks = self.checks(repo, id).await?;
                }
            }
            ("Commit", "commits")
                if id.len() == 40 && id.bytes().all(|b| b.is_ascii_hexdigit()) =>
            {
                // /commits/{sha} includes files/patches; Git commit metadata does not.
                let value = self.get(&format!("repos/{repo}/git/commits/{id}")).await?;
                detail.body = text(&value["message"]);
                detail.author = text(&value["author"]["name"]);
                detail.created_at = text(&value["author"]["date"]);
                detail.url = format!("https://{}/{repo}/commit/{id}", self.host);
                detail.comments = self
                    .pages(&format!("repos/{repo}/commits/{id}/comments"))
                    .await?
                    .iter()
                    .map(comment)
                    .collect();
                detail.unavailable.clear();
            }
            ("Release", "releases") if numeric(id) => {
                detail = Detail::subject(&self.get(&path).await?, thread, &self.host);
            }
            ("Discussion", "discussions") if numeric(id) => {
                let (owner, name) = repo.split_once('/').ok_or("invalid-response")?;
                let value = self.graphql("query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){discussion(number:$number){id title body url createdAt updatedAt closed author{login}}}}",json!({"owner":owner,"name":name,"number":id.parse::<u64>().map_err(|_|"invalid-response")?})).await?;
                let subject = &value["data"]["repository"]["discussion"];
                let node = subject["id"].as_str().ok_or("context-unavailable")?;
                detail.body = text(&subject["body"]);
                detail.author = text(&subject["author"]["login"]);
                detail.created_at = text(&subject["createdAt"]);
                detail.updated_at = text(&subject["updatedAt"]);
                detail.state = if subject["closed"] == true {
                    "closed"
                } else {
                    "open"
                }
                .into();
                detail.url = safe_web(&subject["url"], &self.host).unwrap_or_default();
                detail.comments = self.node_comments(node, "Discussion", "comments").await?;
                let mut replies = vec![];
                for entry in &detail.comments {
                    if let Some(node) = entry["id"].as_str() {
                        replies.extend(
                            self.node_comments(node, "DiscussionComment", "replies")
                                .await?,
                        );
                        bounded(&(&detail, &replies))?;
                    }
                }
                detail.comments.extend(replies);
                detail.unavailable.clear();
            }
            ("CheckSuite", "check-suites") | ("CheckSuite", "check_suites") if numeric(id) => {
                let value = self.get(&format!("repos/{repo}/check-suites/{id}")).await?;
                detail.state = text(&value["status"]);
                detail.body = text(&value["conclusion"]);
                detail.author = text(&value["app"]["name"]);
                detail.checks =
                    json!([{"name":detail.author,"status":detail.state,"conclusion":detail.body}]);
                detail.unavailable="GitHub provides check metadata for this notification; use GitHub for the complete run.".into();
            }
            _ => {}
        }
        bounded(&detail)?;
        Ok(detail)
    }
    async fn node_comments(&self, node: &str, kind: &str, field: &str) -> Result<Vec<Value>> {
        let query=format!("query($id:ID!,$after:String){{node(id:$id){{... on {kind} {{{field}(first:50,after:$after){{nodes{{id body createdAt updatedAt author{{login}}}} pageInfo{{hasNextPage endCursor}}}}}}}}}}");
        let mut cursor = Value::Null;
        let mut entries = vec![];
        for _ in 0..200 {
            let value = self
                .graphql(&query, json!({"id":node,"after":cursor}))
                .await?;
            let connection = &value["data"]["node"][field];
            let batch = connection["nodes"]
                .as_array()
                .ok_or("context-unavailable")?;
            entries.extend(batch.iter().map(|v| json!({"id":v["id"],"body":v["body"],"author":v["author"]["login"],"createdAt":v["createdAt"],"updatedAt":v["updatedAt"]})));
            bounded(&entries)?;
            if connection["pageInfo"]["hasNextPage"] != true {
                return Ok(entries);
            }
            let next = connection["pageInfo"]["endCursor"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("invalid-response")?;
            if cursor == next {
                return Err("invalid-response");
            }
            cursor = json!(next);
        }
        Err("context-too-large")
    }
    async fn checks(&self, repository: &str, number: &str) -> Result<Value> {
        let (owner, name) = repository.split_once('/').ok_or("invalid-response")?;
        let query="query($owner:String!,$name:String!,$number:Int!,$after:String){repository(owner:$owner,name:$name){pullRequest(number:$number){commits(last:1){nodes{commit{statusCheckRollup{state contexts(first:50,after:$after){nodes{... on CheckRun{name status conclusion} ... on StatusContext{context state description}} pageInfo{hasNextPage endCursor}}}}}}}}}";
        let mut cursor = Value::Null;
        let mut checks = vec![];
        for _ in 0..200 {
            let value=self.graphql(query,json!({"owner":owner,"name":name,"number":number.parse::<u64>().map_err(|_|"invalid-response")?,"after":cursor})).await?;
            let rollup = &value["data"]["repository"]["pullRequest"]["commits"]["nodes"][0]
                ["commit"]["statusCheckRollup"];
            if rollup.is_null() {
                return Ok(json!([]));
            }
            let connection = &rollup["contexts"];
            checks.extend(
                connection["nodes"]
                    .as_array()
                    .ok_or("invalid-response")?
                    .iter()
                    .cloned(),
            );
            bounded(&checks)?;
            if connection["pageInfo"]["hasNextPage"] != true {
                return Ok(json!(checks));
            }
            let next = connection["pageInfo"]["endCursor"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("invalid-response")?;
            if cursor == next {
                return Err("invalid-response");
            }
            cursor = json!(next);
        }
        Err("context-too-large")
    }
}
pub fn numeric(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_digit())
}
pub fn component(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 200
        && !matches!(s, "." | "..")
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
pub fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_owned()
}
pub fn subject_path(url: &str, host: &str, repo: &str) -> Option<String> {
    let prefix = if host == "github.com" {
        "https://api.github.com/".into()
    } else {
        format!("https://{host}/api/v3/")
    };
    let path = url.strip_prefix(&prefix)?;
    let parts: Vec<_> = path.split('/').collect();
    (parts.len() == 5
        && parts[0] == "repos"
        && format!("{}/{}", parts[1], parts[2]) == repo
        && parts[1..].iter().all(|s| component(s)))
    .then(|| path.to_owned())
}
pub fn safe_web(value: &Value, host: &str) -> Option<String> {
    let raw = value.as_str()?;
    let url = url::Url::parse(raw).ok()?;
    (raw.len() <= 2048
        && url.scheme() == "https"
        && url.host_str() == Some(host)
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && !raw.chars().any(|c| c.is_control()))
    .then(|| raw.into())
}
fn comment(v: &Value) -> Value {
    json!({"id":v["id"],"author":v["user"]["login"],"body":v["body"],"createdAt":v["created_at"],"updatedAt":v["updated_at"]})
}

fn bounded(value: &impl serde::Serialize) -> Result<()> {
    if serde_json::to_vec(value)
        .map_err(|_| "invalid-response")?
        .len()
        > 2 * 1024 * 1024
    {
        Err("context-too-large")
    } else {
        Ok(())
    }
}
