//! Port inspector presentation: the proposed URL, the failure wording and the
//! bounded action queue.
//!
//! Discovery, ownership and every authorization decision belong to
//! `seele-ports`. What is left here is what the panel needs between two
//! snapshots, and it is kept in Rust so a URL is never assembled by string
//! concatenation in QML.
use crate::value::{array, text, trim};
use serde_json::{Value, json};

/// The only two schemes an inspector is willing to propose. A port number is
/// never proof of either, so the scheme is always visible and changeable.
fn scheme(value: &str) -> Option<&'static str> {
    match value {
        "http" => Some("http"),
        "https" => Some("https"),
        _ => None,
    }
}

/// A destination is the kernel's own rendering of an address, already
/// bracketed when it is IPv6. Anything else is refused rather than escaped,
/// because a host that needs escaping did not come from a socket.
fn destination(value: &str) -> Option<&str> {
    let inner = value
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'));
    let body = inner.unwrap_or(value);
    let allowed = |byte: u8| byte.is_ascii_hexdigit() || byte == b'.' || byte == b':';
    if body.is_empty() || body.len() > 45 || !body.bytes().all(allowed) {
        return None;
    }
    // A colon only belongs inside brackets, or the port separator is ambiguous.
    if body.contains(':') && inner.is_none() {
        return None;
    }
    Some(value)
}

fn failure(code: &str) -> &'static str {
    match code {
        "" => "",
        "gone" => "That listener is gone. The port list has been refreshed.",
        "changed" => {
            "The listener changed while the confirmation was open. Nothing was stopped; review it again."
        }
        "not-authorized" => "Authentication was cancelled or refused. Nothing was stopped.",
        "not-escalatable" => {
            "Try a graceful stop first. Force stop is offered only if it does not work."
        }
        "invalid" => "That target is no longer valid. Refresh and try again.",
        "unavailable" => "This listener has no target Seele can stop.",
        "unknown" => "The owner could not be identified.",
        "failed" => "The stop request failed. The listener is still running.",
        _ => "The action failed. Try again.",
    }
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let first = args.first().unwrap_or(&null);
    Ok(match function {
        // The address a browser should actually be sent to, or an empty string
        // when there is no honest one to propose.
        "url" => {
            let host = text(Some(first));
            let port = args.get(1).and_then(Value::as_u64).unwrap_or(0);
            let chosen = text(args.get(2));
            match (destination(trim(&host)), scheme(&chosen), port) {
                (Some(host), Some(scheme), 1..=65535) => json!(format!("{scheme}://{host}:{port}")),
                _ => json!(""),
            }
        }
        "failure" => json!(failure(&text(Some(first)))),
        // One line naming what the row is, used by the row and repeated in its
        // confirmation so both describe the same thing.
        "summary" => {
            let row = first;
            let owners = array(row.get("owners"));
            let target = row.get("target").unwrap_or(&null);
            let kind = target.get("kind").and_then(Value::as_str).unwrap_or("");
            let name = text(target.get("name"));
            let user = text(target.get("user"));
            let unit = text(target.get("unit"));
            let text = match kind {
                "service" if !unit.is_empty() => unit,
                "process" if !name.is_empty() => {
                    let pid = target.get("pid").and_then(Value::as_u64).unwrap_or(0);
                    if user.is_empty() {
                        format!("{name} ({pid})")
                    } else {
                        format!("{name} ({pid}) · {user}")
                    }
                }
                "ambiguous" => format!("{} processes", owners.len()),
                _ => {
                    let owner = text(row.get("user"));
                    if owner.is_empty() {
                        "Unknown owner".to_owned()
                    } else {
                        format!("Unknown process · {owner}")
                    }
                }
            };
            json!(text)
        }
        // A pending action is one action. The panel may not stack requests on
        // a listener whose identity it is still waiting to re-read.
        "pending" => {
            let queue = array(Some(first));
            let value = args.get(1).unwrap_or(&null);
            if queue.len() >= 8 {
                json!({"error": "Too many pending actions. Wait for the current one."})
            } else if queue.contains(value) {
                Value::Null
            } else {
                let mut queue = queue.to_vec();
                queue.push(value.clone());
                json!({"queue": queue})
            }
        }
        _ => return Err("unknown Ports UI function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::string;

    fn url(host: &str, port: u64, scheme: &str) -> String {
        string(Some(
            &call("url", &[json!(host), json!(port), json!(scheme)]).unwrap(),
        ))
    }

    #[test]
    fn a_proposed_url_is_valid_for_both_families_or_is_not_proposed() {
        assert_eq!(url("127.0.0.1", 3000, "http"), "http://127.0.0.1:3000");
        assert_eq!(
            url("192.168.1.10", 8443, "https"),
            "https://192.168.1.10:8443"
        );
        assert_eq!(
            url("[::1]", 9000, "http"),
            "http://[::1]:9000",
            "an IPv6 host keeps its brackets so the port stays readable"
        );
        assert_eq!(url("[fd00::5]", 80, "http"), "http://[fd00::5]:80");
        for (host, port, scheme) in [
            ("127.0.0.1", 0, "http"),
            ("127.0.0.1", 65536, "http"),
            ("127.0.0.1", 3000, "file"),
            ("127.0.0.1", 3000, "javascript"),
            ("::1", 3000, "http"),
            ("example.test/../x", 3000, "http"),
            ("127.0.0.1 -x", 3000, "http"),
            ("", 3000, "http"),
        ] {
            assert_eq!(url(host, port, scheme), "", "{host}:{port} {scheme}");
        }
    }

    #[test]
    fn a_row_describes_its_own_target() {
        let service =
            json!({"user":"root","owners":[{}],"target":{"kind":"service","unit":"nginx.service"}});
        assert_eq!(
            string(Some(&call("summary", &[service]).unwrap())),
            "nginx.service"
        );
        let process = json!({"user":"silash","owners":[{}],
            "target":{"kind":"process","name":"node","pid":101,"user":"silash"}});
        assert_eq!(
            string(Some(&call("summary", &[process]).unwrap())),
            "node (101) · silash"
        );
        let shared = json!({"user":"silash","owners":[{},{}],"target":{"kind":"ambiguous"}});
        assert_eq!(
            string(Some(&call("summary", &[shared]).unwrap())),
            "2 processes"
        );
        let hidden = json!({"user":"root","owners":[],"target":{"kind":"none"}});
        assert_eq!(
            string(Some(&call("summary", &[hidden]).unwrap())),
            "Unknown process · root",
            "an unreadable owner is named as unknown rather than invented"
        );
    }

    #[test]
    fn every_refusal_has_wording_that_does_not_claim_success() {
        for code in [
            "gone",
            "changed",
            "not-authorized",
            "not-escalatable",
            "failed",
        ] {
            let text = string(Some(&call("failure", &[json!(code)]).unwrap()));
            assert!(!text.is_empty(), "{code} needs wording");
            assert!(
                !text.to_lowercase().contains("stopped successfully"),
                "{code} must not read as a success"
            );
        }
        assert_eq!(string(Some(&call("failure", &[json!("")]).unwrap())), "");
        assert!(call("nonsense", &[]).is_err());
    }

    #[test]
    fn the_action_queue_stays_bounded_and_collapses_repeats() {
        let queue = json!([]);
        let action = json!({"op":"stop","token":"1|41|x|1|process|||1|1|0"});
        let next = call("pending", &[queue, action.clone()]).unwrap();
        assert_eq!(next["queue"].as_array().unwrap().len(), 1);
        assert!(
            call("pending", &[next["queue"].clone(), action.clone()])
                .unwrap()
                .is_null(),
            "the same confirmed action is never sent twice"
        );
        let full = json!(vec![json!({"op": "refresh"}); 8]);
        assert!(call("pending", &[full, action]).unwrap()["error"].is_string());
    }
}
