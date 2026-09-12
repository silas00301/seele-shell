//! Bounded read-only GitHub snapshot. The CLI owns authentication; the UI sees
//! only fixed public errors, never CLI diagnostics or credentials.
use crate::process::{capture, Limits};
use serde_json::{json, Value};
use std::{collections::HashSet, process::Command, sync::atomic::AtomicUsize, time::Duration};

const QUERY: &str = include_str!("github.graphql");
#[derive(Debug)]
pub struct Failure {
    state: &'static str,
    message: &'static str,
}
impl Failure {
    fn error(message: &'static str) -> Self {
        Self {
            state: "error",
            message,
        }
    }
    fn value(&self) -> Value {
        json!({"state": self.state, "message": self.message})
    }
}
fn failure(text: &str) -> Failure {
    let lower = text.to_ascii_lowercase();
    if ["rate limit", "rate_limit", "http 429"]
        .iter()
        .any(|s| lower.contains(s))
    {
        Failure {
            state: "rate-limited",
            message: "GitHub's rate limit was reached. Automatic refresh will wait five minutes.",
        }
    } else if [
        "http 401",
        "bad credentials",
        "authentication",
        "gh auth login",
        "not logged",
    ]
    .iter()
    .any(|s| lower.contains(s))
    {
        Failure {
            state: "auth-required",
            message: "Sign in with GitHub CLI, then refresh.",
        }
    } else {
        Failure::error(
            "GitHub could not be reached. Check your connection and account access, then refresh.",
        )
    }
}
fn hostname(value: &str) -> Result<String, Failure> {
    let host = value.to_ascii_lowercase();
    if host.is_empty()
        || host.len() > 253
        || host.contains("..")
        || !host.as_bytes()[0].is_ascii_alphanumeric()
        || !host.as_bytes()[host.len() - 1].is_ascii_alphanumeric()
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-'))
    {
        return Err(Failure::error("The configured GitHub hostname is invalid."));
    }
    Ok(host)
}
/// Canonical pull-request URL grammar shared by source collection and UI intent.
pub fn safe_url<'a>(value: &'a Value, host: &str) -> Option<&'a str> {
    if hostname(host).ok()?.as_str() != host {
        return None;
    }
    let url = value.as_str()?;
    if url.len() > 1024 {
        return None;
    }
    // Literal authority and path reject URL parser normalization discrepancies.
    let path = url
        .strip_prefix("https://")?
        .strip_prefix(host)?
        .strip_prefix('/')?;
    let parts: Vec<_> = path.split('/').collect();
    if parts.len() != 4
        || parts[2] != "pull"
        || parts[3].starts_with('0')
        || parts[3].is_empty()
        || !parts[3].bytes().all(|c| c.is_ascii_digit())
        || !parts[..2].iter().all(|part| {
            !part.is_empty()
                && !matches!(*part, "." | "..")
                && part
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
        })
    {
        return None;
    }
    Some(url)
}
fn plain(value: &Value, limit: usize) -> String {
    value.as_str().unwrap_or("").chars().take(limit).map(|c| {
        if c.is_control() || matches!(c, '\u{00ad}' | '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}') { ' ' } else { c }
    }).collect()
}
fn pull(item: &Value, host: &str) -> Option<Value> {
    let url = safe_url(&item["url"], host)?;
    let number = item["number"].as_u64().filter(|number| *number > 0)?;
    let check = item["commits"]["nodes"]
        .as_array()
        .and_then(|nodes| nodes.last())
        .and_then(|node| node["commit"]["statusCheckRollup"]["state"].as_str())
        .unwrap_or("UNKNOWN");
    let checks = if ["SUCCESS", "FAILURE", "PENDING", "ERROR", "EXPECTED"].contains(&check) {
        check
    } else {
        "UNKNOWN"
    };
    let review = item["reviewDecision"].as_str().unwrap_or("");
    let review = if ["APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED"].contains(&review) {
        review
    } else {
        ""
    };
    Some(
        json!({"number": number, "title": plain(&item["title"], 256), "url": url,
        "repository": plain(&item["repository"]["nameWithOwner"], 200), "updatedAt": plain(&item["updatedAt"], 32),
        "draft": item["isDraft"] == true, "checks": checks, "review": review}),
    )
}
fn connection(value: &Value, host: &str, count: &str) -> Result<(Vec<Value>, u64), Failure> {
    let nodes = value["nodes"]
        .as_array()
        .ok_or_else(|| Failure::error("GitHub returned incomplete pull-request data."))?;
    let mut seen = HashSet::new();
    let entries: Vec<_> = nodes
        .iter()
        .take(20)
        .filter_map(|item| pull(item, host))
        .filter(|item| seen.insert(item["url"].as_str().unwrap().to_owned()))
        .collect();
    let total = value[count].as_u64().unwrap_or(0).max(entries.len() as u64);
    Ok((entries, total))
}
fn snapshot(
    mut run: impl FnMut(&[String]) -> Result<Value, Failure>,
    host: &str,
) -> Result<Value, Failure> {
    let host = hostname(host)?;
    let identity = run(&[
        "api".into(),
        "--hostname".into(),
        host.clone(),
        "user".into(),
    ])?;
    let login = identity["login"]
        .as_str()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 64
                && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
        .ok_or_else(|| Failure::error("GitHub did not identify the current account."))?;
    let result = run(&[
        "api".into(),
        "--hostname".into(),
        host.clone(),
        "graphql".into(),
        "-f".into(),
        format!("query={QUERY}"),
        "-f".into(),
        format!("reviews=is:pr is:open review-requested:{login} sort:updated-desc"),
    ])?;
    if let Some(errors) = result["errors"]
        .as_array()
        .filter(|errors| !errors.is_empty())
    {
        return Err(failure(
            &errors
                .iter()
                .filter_map(|item| item["message"].as_str())
                .collect::<Vec<_>>()
                .join(" "),
        ));
    }
    let data = &result["data"];
    if data["viewer"]["login"].as_str() != Some(login) {
        return Err(Failure::error(
            "The GitHub account changed during refresh. Refresh again.",
        ));
    }
    let (authored, authored_total) =
        connection(&data["viewer"]["pullRequests"], &host, "totalCount")?;
    let (reviews, review_total) = connection(&data["search"], &host, "issueCount")?;
    Ok(
        json!({"state": "ready", "message": "", "host": host, "viewer": login,
        "updatedAt": crate::time::timestamp(), "authored": authored, "reviews": reviews,
        "authoredTotal": authored_total, "reviewTotal": review_total}),
    )
}
pub fn run(cancelled: &AtomicUsize) -> Value {
    snapshot(|args| {
        let output = capture(Command::new("gh").args(args).env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat").env("NO_COLOR", "1").env_remove("GH_DEBUG"), b"",
            Limits { timeout: Duration::from_secs(15), output: 256 * 1024 }, cancelled).map_err(|error| {
                use std::io::ErrorKind;
                Failure::error(match error.kind() {
                    ErrorKind::NotFound => "GitHub CLI is unavailable.",
                    ErrorKind::TimedOut => "GitHub took too long to respond. Try refreshing again.",
                    ErrorKind::InvalidData => "GitHub returned more data than this panel can display.",
                    _ => "GitHub could not be reached. Check your connection and account access, then refresh.",
                })
            })?;
        if !output.status.success() { return Err(failure(&String::from_utf8_lossy(&output.stderr))); }
        serde_json::from_slice(&output.stdout).map_err(|_| Failure::error("GitHub returned an unreadable response."))
    }, &std::env::var("SEELE_GITHUB_HOST").unwrap_or_else(|_| "github.com".into())).unwrap_or_else(|error| error.value())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls_reject_authority_confusion_and_path_normalization() {
        let valid = json!("https://github.com/org/repo/pull/1");
        assert_eq!(safe_url(&valid, "github.com"), valid.as_str());
        for invalid in [
            "https://github.com.evil/org/repo/pull/1",
            "https://user@github.com/org/repo/pull/1",
            "https://github.com:443/org/repo/pull/1",
            "https://github.com/../repo/pull/1",
            "https://github.com/org/repo/pull/0",
            "https://github.com/org/repo/pull/1?secret",
        ] {
            assert!(
                safe_url(&json!(invalid), "github.com").is_none(),
                "{invalid}"
            );
        }
    }
    #[test]
    fn read_only_snapshot_preserves_the_ui_contract() {
        let row = json!({"number": 1, "url": "https://github.com/org/repo/pull/1", "title": "<b>test</b>\nchange",
            "repository": {"nameWithOwner": "org/repo"}, "isDraft": true,
            "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "SUCCESS"}}}]}});
        let mut calls = Vec::new();
        let result = snapshot(|args| {
            calls.push(args.to_vec());
            Ok(if calls.len() == 1 { json!({"login": "fixture"}) } else {
                json!({"data": {"viewer": {"login": "fixture", "pullRequests": {"nodes": [row, row], "totalCount": 3}},
                    "search": {"nodes": [], "issueCount": 0}}})
            })
        }, "github.com").unwrap();
        assert_eq!(calls.len(), 2);
        assert!(!QUERY.contains("mutation"));
        assert_eq!(result["authored"].as_array().unwrap().len(), 1);
        assert_eq!(result["authored"][0]["checks"], "SUCCESS");
        assert_eq!(result["authored"][0]["title"], "<b>test</b> change");
        assert_eq!(result["authoredTotal"], 3);
    }
    #[test]
    fn hostile_shapes_are_rejected_without_panics_or_diagnostics() {
        assert!(pull(
            &json!({"number": 1, "url": "https://github.com/o/r/pull/1", "commits": 3}),
            "github.com"
        )
        .is_some());
        assert!(connection(&json!({"nodes": false}), "github.com", "count").is_err());
        assert_eq!(failure("HTTP 401 SECRET").state, "auth-required");
        assert!(!failure("HTTP 401 SECRET").message.contains("SECRET"));
        for host in ["", "https://github.com", "a..b", "github.com/path"] {
            assert!(hostname(host).is_err());
        }
    }
}
