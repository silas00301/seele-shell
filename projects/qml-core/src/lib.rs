//! Pure UI algorithms behind a narrow, in-process Qt ABI. Qt object identity,
//! signals, animations and extension callbacks remain with their owning host.
use serde::Deserialize;
use serde_json::{Value, json};

mod ai_activity;
mod ai_prompt;
mod focus;
mod github;
mod health;
mod home_assistant;
mod media;
mod models;
mod network;
mod notes;
mod notifications;
mod pi;
mod presentation;
mod system;
mod time;
mod transfers;
mod uri_picker;
pub mod value;
mod vicinae;

pub const MAX_MESSAGE: usize = 16 * 1024 * 1024;
#[unsafe(no_mangle)]
pub extern "C" fn seele_core_max_message() -> usize {
    MAX_MESSAGE
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    operation: String,
    arguments: Vec<Value>,
}

pub fn call(operation: &str, arguments: &[Value]) -> Result<Value, String> {
    if operation.len() > 128 || arguments.len() > 32 {
        return Err("native function request exceeds its limit".into());
    }
    let (module, function) = operation
        .split_once('.')
        .ok_or("invalid native operation")?;
    match module {
        "pi" => pi::call(function, arguments),
        "vicinae" => vicinae::call(function, arguments),
        "notifications" => notifications::call(function, arguments),
        "github" => github::call(function, arguments),
        "health" => health::call(function, arguments),
        "media" => media::call(function, arguments),
        "home_assistant" => home_assistant::call(function, arguments),
        "transfers" => transfers::call(function, arguments),
        "models" => models::call(function, arguments),
        "system" => system::call(function, arguments),
        "presentation" => presentation::call(function, arguments),
        "notes" => notes::call(function, arguments),
        "ai_activity" => ai_activity::call(function, arguments),
        "network" => network::call(function, arguments),
        "ai_prompt" => ai_prompt::call(function, arguments),
        "uri_picker" => uri_picker::call(function, arguments),
        "focus" => focus::call(function, arguments),
        "time" => time::call(function, arguments),
        // Dispatch entries are added with each independently migrated policy.
        "fixture" if function == "echo" => Ok(arguments.first().cloned().unwrap_or(Value::Null)),
        _ => Err("unknown native function".into()),
    }
}

pub fn evaluate(bytes: &[u8]) -> Vec<u8> {
    let result = if bytes.len() > MAX_MESSAGE {
        Err("native function request exceeds its limit".to_owned())
    } else {
        serde_json::from_slice::<Request>(bytes)
            .map_err(|_| "invalid native function request".to_owned())
            .and_then(|request| call(&request.operation, &request.arguments))
    };
    envelope(result)
}
fn envelope(result: Result<Value, String>) -> Vec<u8> {
    let envelope = match result {
        Ok(value) => json!({"ok":true,"value":value}),
        Err(error) => json!({"ok":false,"error":error}),
    };
    seele_runtime::wire::json_frame(&envelope, MAX_MESSAGE).unwrap_or_else(|_| {
        b"{\"ok\":false,\"error\":\"native result exceeds its limit\"}\n".to_vec()
    })
}

#[repr(C)]
pub struct Bytes {
    pub data: *mut u8,
    pub length: usize,
}

/// # Safety
/// `data` must point to `length` live readable bytes for this synchronous call.
/// Release the returned allocation exactly once with `seele_qml_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seele_qml_call(data: *const u8, length: usize) -> Bytes {
    let bytes = if data.is_null() || length > MAX_MESSAGE {
        evaluate(&[])
    } else {
        // SAFETY: caller provides its live QByteArray, with explicit byte count.
        let input = unsafe { std::slice::from_raw_parts(data, length) };
        std::panic::catch_unwind(|| evaluate(input))
            .unwrap_or_else(|_| b"{\"ok\":false,\"error\":\"native function failed\"}\n".to_vec())
    };
    into_bytes(bytes)
}
fn into_bytes(bytes: Vec<u8>) -> Bytes {
    let bytes = bytes.into_boxed_slice();
    let length = bytes.len();
    Bytes {
        data: Box::into_raw(bytes).cast(),
        length,
    }
}

/// # Safety
/// `bytes` must be an unmodified result from `seele_qml_call`, not yet freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seele_qml_free(bytes: Bytes) {
    if !bytes.data.is_null() {
        // SAFETY: reconstruct exactly the owned boxed slice returned above.
        unsafe {
            drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                bytes.data,
                bytes.length,
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundary_is_strict_bounded_and_round_trips_unicode() {
        let result: Value = serde_json::from_slice(&evaluate(
            br#"{"operation":"fixture.echo","arguments":["\ud83e\udd80"]}"#,
        ))
        .unwrap();
        assert_eq!(result, json!({"ok":true,"value":"🦀"}));
        assert_eq!(
            serde_json::from_slice::<Value>(&evaluate(
                br#"{"operation":"fixture.echo","arguments":[],"extra":1}"#
            ))
            .unwrap()["ok"],
            false
        );
        assert!(call("fixture.echo", &vec![Value::Null; 33]).is_err());
    }
}

/// # Safety
/// The returned opaque pointer must be freed once, and used only synchronously
/// from its owning QObject thread. A null result denotes an invalid timestamp.
#[unsafe(no_mangle)]
pub extern "C" fn seele_notifications_new(now: f64) -> *mut std::ffi::c_void {
    notifications::new_state(now).map_or(std::ptr::null_mut(), |state| Box::into_raw(state).cast())
}

/// # Safety
/// `state` must be a live pointer returned by `seele_notifications_new`, with
/// exclusive access during this call. Input and output obey `seele_qml_call`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seele_notifications_call(
    state: *mut std::ffi::c_void,
    data: *const u8,
    length: usize,
) -> Bytes {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if state.is_null() || data.is_null() || length > MAX_MESSAGE {
            return Err("invalid notification request".to_owned());
        }
        // SAFETY: the QObject owns this box and passes a live bounded QByteArray.
        let (state, input) = unsafe {
            (
                &mut *state.cast::<notifications::State>(),
                std::slice::from_raw_parts(data, length),
            )
        };
        let request: Request =
            serde_json::from_slice(input).map_err(|_| "invalid notification request".to_owned())?;
        if request.operation.len() > 128 || request.arguments.len() > 32 {
            return Err("notification request exceeds its limit".into());
        }
        notifications::state_call(state, &request.operation, &request.arguments)
    }))
    .unwrap_or_else(|_| Err("notification state operation failed".into()));
    into_bytes(envelope(result))
}

/// # Safety
/// `state` must be null or a live pointer returned by `seele_notifications_new`,
/// not yet freed; no operation may overlap destruction.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seele_notifications_free(state: *mut std::ffi::c_void) {
    if !state.is_null() {
        // SAFETY: reconstruct the unique box owned by the destroying QObject.
        unsafe {
            drop(Box::from_raw(state.cast::<notifications::State>()));
        }
    }
}

/// Test CLI only: replay bounded events against the same resident state methods.
/// Qt exposes no replay operation, and production never keeps an event log.
pub fn notification_fixture(input: &[u8]) -> Vec<u8> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fixture {
        now: f64,
        steps: Vec<Request>,
    }
    let result = (|| {
        if input.len() > MAX_MESSAGE {
            return Err("fixture exceeds its limit".into());
        }
        let fixture: Fixture =
            serde_json::from_slice(input).map_err(|_| "invalid notification fixture".to_owned())?;
        if fixture.steps.len() > 1024 {
            return Err("fixture exceeds its limit".into());
        }
        let mut state =
            notifications::new_state(fixture.now).ok_or("invalid notification timestamp")?;
        let mut result = Value::Null;
        for step in fixture.steps {
            if step.operation.len() > 128 || step.arguments.len() > 32 {
                return Err("fixture exceeds its limit".into());
            }
            result = notifications::state_call(&mut state, &step.operation, &step.arguments)?;
        }
        Ok(result)
    })();
    envelope(result)
}

#[cfg(test)]
mod ffi_tests {
    use super::*;
    fn take(bytes: Bytes) -> Value {
        assert!(bytes.length <= MAX_MESSAGE);
        // SAFETY: inspect and free exactly one freshly returned Rust allocation.
        unsafe {
            let value =
                serde_json::from_slice(std::slice::from_raw_parts(bytes.data, bytes.length))
                    .unwrap();
            seele_qml_free(bytes);
            value
        }
    }
    fn state_request(state: *mut std::ffi::c_void, operation: &str, arguments: Value) -> Value {
        let input =
            serde_json::to_vec(&json!({"operation":operation,"arguments":arguments})).unwrap();
        // SAFETY: tests own the live state, synchronously, and this input buffer.
        take(unsafe { seele_notifications_call(state, input.as_ptr(), input.len()) })
    }
    #[test]
    fn ffi_rejects_null_empty_oversize_depth_and_invalid_envelopes() {
        // SAFETY: rejected null/oversized buffers are never dereferenced.
        for length in [0, MAX_MESSAGE + 1, usize::MAX] {
            assert_eq!(
                take(unsafe { seele_qml_call(std::ptr::null(), length) })["ok"],
                false
            );
        }
        for input in [
            Vec::new(),
            b"{}".to_vec(),
            br#"{"operation":"fixture.echo","arguments":[],"extra":true}"#.to_vec(),
            serde_json::to_vec(
                &json!({"operation":"fixture.echo","arguments":vec![Value::Null;33]}),
            )
            .unwrap(),
            format!(
                "{{\"operation\":\"fixture.echo\",\"arguments\":[{}0{}]}}",
                "[".repeat(200),
                "]".repeat(200)
            )
            .into_bytes(),
        ] {
            // SAFETY: live input slice; returned allocation is consumed once.
            assert_eq!(
                take(unsafe { seele_qml_call(input.as_ptr(), input.len()) })["ok"],
                false
            );
        }
        for value in [
            json!(""),
            json!("🦀\u{0000} private stays local"),
            json!([true,null,{"k":"v"}]),
        ] {
            let input =
                serde_json::to_vec(&json!({"operation":"fixture.echo","arguments":[value]}))
                    .unwrap();
            // SAFETY: live input slice; returned allocation is consumed once.
            assert_eq!(
                take(unsafe { seele_qml_call(input.as_ptr(), input.len()) }),
                json!({"ok":true,"value":value})
            );
        }
    }
    #[test]
    fn resident_stores_are_isolated_restore_and_mutate_before_effects() {
        let first = seele_notifications_new(1000.0);
        let second = seele_notifications_new(1000.0);
        assert!(!first.is_null() && !second.is_null());
        assert!(seele_notifications_new(f64::NAN).is_null());
        let entry = json!({"id":7,"summary":"🦀","body":"local","timeout":-1,"urgency":1,"transient":false,"time":1000,"actions":{},"pinned":false});
        let received = state_request(first, "receive", json!([entry, 1000, false]));
        assert_eq!(received["ok"], true);
        assert_eq!(received["value"]["effects"][0]["operation"], "arrived");
        // A callback can immediately read/close: mutation precedes every effect.
        assert_eq!(state_request(first, "view", json!([]))["value"]["count"], 1);
        assert_eq!(
            state_request(second, "view", json!([]))["value"]["count"],
            0
        );
        assert_eq!(
            state_request(first, "pin", json!([7]))["value"]["result"],
            true
        );
        let saved = state_request(first, "save", json!([]))["value"].clone();
        assert_eq!(saved["metadata"]["7"]["pinned"], true);
        assert_eq!(state_request(second, "restore", json!([saved]))["ok"], true);
        state_request(second, "receive", json!([entry, 1005, true]));
        assert_eq!(
            state_request(second, "view", json!([]))["value"]["items"][0]["pinned"],
            true
        );
        state_request(first, "closed", json!([7, 2]));
        assert_eq!(
            state_request(first, "view", json!([]))["value"]["history"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            state_request(second, "view", json!([]))["value"]["count"],
            1
        );
        // SAFETY: each uniquely owned state is released once, after all calls.
        unsafe {
            seele_notifications_free(first);
            seele_notifications_free(second);
            seele_notifications_free(std::ptr::null_mut());
        }
    }
}
