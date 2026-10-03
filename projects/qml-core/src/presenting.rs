//! Presentation mode: what the shell holds back while a screen is being shown
//! to other people, and which keep-awake session it may end afterwards.
//!
//! The mode borrows existing owners rather than taking them over. Do Not Disturb
//! is never written, so ending the mode has nothing to restore there, and the
//! Caffeinate service keeps its single session: the mode starts one only when
//! none is running and ends only the one it started.
use crate::value::{number, string, truthy};
use serde_json::{Value, json};

// The watcher reports an untimed session only on its 20-second heartbeat, so the
// elapsed time it last said can trail the real one by that much and a little.
const STALE: f64 = 25.0;
const AHEAD: f64 = 5.0;

// Hidden while presenting by hand or while the screen is being shared, because
// a share is exactly when a title on the bar reaches someone else.
fn state(manual: bool, sharing: bool) -> Value {
    let (active, label, detail) = match (manual, sharing) {
        (true, _) => (
            true,
            "Presenting",
            "Toasts and personal bar details are hidden, and the session stays awake",
        ),
        (false, true) => (
            true,
            "Sharing",
            "Screen is shared, so toasts and personal bar details are hidden",
        ),
        _ => (false, "", ""),
    };
    json!({"active":active,"manual":manual,"label":label,"detail":detail})
}
// Whether starting the mode should start a keep-awake session: never over one
// the user already has, which would replace it and then end it with the mode.
fn claim(session: &Value) -> bool {
    !truthy(session.get("active"))
}
// Whether the running session is still the one the mode started at `started`.
// A session the user started or replaced since then has a younger elapsed time.
fn owns(started: f64, session: &Value, now: f64) -> bool {
    if !(started.is_finite() && started > 0.0 && now.is_finite()) {
        return false;
    }
    if !truthy(session.get("active")) || string(session.get("mode")) != "manual" {
        return false;
    }
    let elapsed = number(session.get("elapsed"));
    let expected = now - started;
    elapsed.is_finite() && elapsed >= expected - STALE && elapsed <= expected + AHEAD
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    Ok(match function {
        "state" => state(truthy(args.first()), truthy(args.get(1))),
        "claim" => json!(claim(args.first().unwrap_or(&null))),
        "owns" => json!(owns(
            number(args.first()),
            args.get(1).unwrap_or(&null),
            number(args.get(2))
        )),
        _ => return Err("unknown presenting function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sharing_hides_like_presenting_without_keeping_awake() {
        assert_eq!(state(false, false)["active"], false);
        assert_eq!(state(true, false)["label"], "Presenting");
        let shared = state(false, true);
        assert_eq!(
            (shared["active"].clone(), shared["manual"].clone()),
            (json!(true), json!(false))
        );
        assert_eq!(
            state(true, true)["label"],
            "Presenting",
            "a chosen mode outranks the share"
        );
    }
    #[test]
    fn a_session_the_user_owns_is_never_claimed_or_ended() {
        assert!(claim(&json!({"active":false})));
        assert!(!claim(
            &json!({"active":true,"mode":"duration","remaining":600})
        ));
        let ours = json!({"active":true,"mode":"manual","elapsed":600});
        assert!(owns(1000.0, &ours, 1600.0));
        assert!(
            owns(
                1000.0,
                &json!({"active":true,"mode":"manual","elapsed":580}),
                1600.0
            ),
            "a heartbeat-stale snapshot is still ours"
        );
        assert!(
            !owns(
                1000.0,
                &json!({"active":true,"mode":"manual","elapsed":60}),
                1600.0
            ),
            "replaced since"
        );
        assert!(!owns(
            1000.0,
            &json!({"active":true,"mode":"duration","elapsed":600}),
            1600.0
        ));
        assert!(!owns(1000.0, &json!({"active":false}), 1600.0));
        assert!(!owns(0.0, &ours, 1600.0), "the mode started nothing");
    }
}
