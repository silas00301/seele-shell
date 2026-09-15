//! Microphone-test presentation, device resolution and the explicit gate in
//! front of a test that would take a microphone another application is holding.
//!
//! The worker owns the audio; this owns what the panel says about it. Levels
//! arrive already measured from the captured samples, so the bar drawn here
//! cannot decide whether the signal clipped — it only reports the answer the
//! capture gave.
use crate::value::{array, finite, fixed, number, text, truthy};
use serde_json::{Value, json};

/// The Audio panel lists at most a handful of outputs; the bound exists so a
/// malformed device snapshot cannot become an unbounded projection.
const MAX_DEVICES: usize = 64;
const MAX_USERS: usize = 16;

fn phrase(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("{one} is using the microphone"),
        [one, two] => format!("{one} and {two} are using the microphone"),
        [one, two, rest @ ..] => format!(
            "{one}, {two} and {} more are using the microphone",
            rest.len()
        ),
    }
}

fn names(value: Option<&Value>) -> Vec<String> {
    array(value)
        .iter()
        .take(MAX_USERS)
        .map(|value| text(Some(value)))
        .filter(|value| !value.is_empty())
        .collect()
}

/// Who else is recording. "Nobody is using it" and "nobody could be asked" are
/// different answers, and only the first of them is reassuring.
fn report(value: Option<&Value>, detection: bool) -> Value {
    let names = names(value);
    if !detection {
        return json!({
            "known": false,
            "busy": false,
            "title": "Microphone use cannot be checked",
            "detail": "The audio server did not report which applications are recording.",
        });
    }
    if names.is_empty() {
        return json!({"known":true,"busy":false,"title":"","detail":""});
    }
    json!({
        "known": true,
        "busy": true,
        "title": phrase(&names),
        "detail": "The test records alongside it; nothing is muted, stopped or rerouted.",
    })
}

fn mode(status: &Value) -> String {
    let mode = text(status.get("mode"));
    match mode.as_str() {
        "recording" | "playing" | "live" => mode,
        _ => "idle".to_owned(),
    }
}

/// One output row the panel can offer for the test, named by its PipeWire node
/// because that is what the worker resolves and what routes one stream.
fn outputs(devices: &[Value]) -> Vec<Value> {
    devices
        .iter()
        .take(MAX_DEVICES)
        .filter(|device| text(device.get("kind")) == "output")
        .filter(|device| !text(device.get("node")).is_empty())
        .map(|device| {
            json!({
                "node": text(device.get("node")),
                "name": text(device.get("name")),
                "system": truthy(device.get("selected")) || truthy(device.get("default")),
            })
        })
        .collect()
}

fn present(options: &[Value], node: &str) -> bool {
    !node.is_empty() && options.iter().any(|option| option["node"] == node)
}

fn default_input(devices: &[Value]) -> Option<&Value> {
    devices
        .iter()
        .take(MAX_DEVICES)
        .filter(|device| text(device.get("kind")) == "input")
        .find(|device| truthy(device.get("default")))
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let first = args.first().unwrap_or(&null);
    Ok(match function {
        // The test output defaults to the system's own selection and is only
        // ever a name for this test's stream: nothing here changes a default
        // or moves another application.
        "devices" => {
            let devices = array(Some(first));
            if devices.len() > 4096 {
                return Err("audio device limit exceeded".into());
            }
            let options = outputs(devices);
            let chosen = text(args.get(1));
            let system = options
                .iter()
                .find(|option| truthy(option.get("system")))
                .or_else(|| options.first());
            let output = if present(&options, &chosen) {
                chosen.clone()
            } else {
                system.map_or(String::new(), |option| text(option.get("node")))
            };
            let input = default_input(devices);
            let status = args.get(2).unwrap_or(&null);
            let running = mode(status) != "idle";
            // A device that disappears under a running test ends it. Live
            // playback is never quietly moved to whatever is left.
            let testing = text(status.get("input"));
            let holds_input = devices.iter().take(MAX_DEVICES).any(|device| {
                text(device.get("kind")) == "input" && text(device.get("node")) == testing
            });
            let lost = if !running {
                ""
            } else if !testing.is_empty() && !holds_input {
                "input"
            } else if !present(&options, &text(status.get("output"))) {
                "output"
            } else {
                ""
            };
            let message = match lost {
                "input" => {
                    "The microphone being tested is no longer available. Choose another and start again."
                }
                "output" => "The test output is no longer available. Choose another and start again.",
                _ => "",
            };
            let output_name = options
                .iter()
                .find(|option| text(option.get("node")) == output)
                .map_or(String::new(), |option| text(option.get("name")));
            let input_node = input.map_or(String::new(), |device| text(device.get("node")));
            let input_name = input.map_or(String::new(), |device| text(device.get("name")));
            json!({
                "options": options,
                "output": output,
                "outputName": output_name,
                "input": input_node,
                "inputName": input_name,
                "lost": lost,
                "message": message,
            })
        }
        "users" => report(Some(first), truthy(args.get(1))),
        // Everything the card draws, derived whole so a stale frame cannot
        // leave the meter disagreeing with the state above it.
        "view" => {
            let status = first;
            let meter = args.get(1).unwrap_or(&null);
            let mode = mode(status);
            let (recording, playing, live) =
                (mode == "recording", mode == "playing", mode == "live");
            let active = mode != "idle";
            let capturing = recording || live;
            let error = text(status.get("error"));
            let sample = truthy(status.get("sample"));
            let remaining = finite(meter.get("remaining"), -1.0);
            let left = if recording && remaining >= 0.0 {
                format!("{} s left", fixed((remaining / 1000.0).max(0.0), 1))
            } else {
                String::new()
            };
            // The meter reads zero whenever nothing is being captured, rather
            // than holding the last frame of a finished recording.
            let level = if capturing {
                number(meter.get("level")).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let peak = if capturing {
                number(meter.get("peak")).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let title = if recording {
                "Recording"
            } else if playing {
                "Playing back"
            } else if live {
                "Live"
            } else {
                "Microphone test"
            };
            let detail = if !error.is_empty() {
                error.clone()
            } else if recording {
                left.clone()
            } else if playing {
                "Playing the five-second sample".to_owned()
            } else if live {
                "The microphone is playing through the test output".to_owned()
            } else if sample {
                "A five-second sample is ready to replay".to_owned()
            } else {
                "Record five seconds, or listen live".to_owned()
            };
            let tone = if !error.is_empty() {
                "error"
            } else if recording {
                "recording"
            } else if playing {
                "playing"
            } else if live {
                "live"
            } else {
                "idle"
            };
            json!({
                "mode": mode,
                "active": active,
                "recording": recording,
                "playing": playing,
                "live": live,
                "level": level,
                "peak": peak,
                "clipped": capturing && truthy(meter.get("clipped")),
                "remaining": left,
                "title": title,
                "detail": detail,
                "tone": tone,
                // Mute is reported rather than corrected, and only while a
                // test is running, because that is when it was resolved.
                "muted": active && truthy(status.get("muted")),
                "sample": sample,
                "sampleClipped": sample && !active && truthy(status.get("clipped")),
                "canRecord": !active,
                "canLive": !active,
                "canReplay": !active && sample,
                "canStop": active,
                "error": error,
            })
        }
        // Both modes pass through here, so the microphone-use gate cannot be
        // reached only by one of them.
        "start" => {
            let action = text(first);
            if action != "sample" && action != "live" {
                return Err("unknown microphone test mode".into());
            }
            let input = text(args.get(1));
            let output = text(args.get(2));
            if input.is_empty() {
                return Ok(json!({"error":"Choose a microphone in the Audio panel first."}));
            }
            if output.is_empty() {
                return Ok(json!({"error":"Choose an output for the test first."}));
            }
            let report = report(args.get(3), truthy(args.get(4)));
            // Confirmation is required for use that was actually observed. An
            // unavailable check is reported instead of being turned into a
            // prompt that could never be answered from evidence.
            if truthy(report.get("busy")) && !truthy(args.get(5)) {
                return Ok(json!({"confirm":report,"action":action}));
            }
            json!({"command":{"command":action,"input":input,"output":output}})
        }
        _ => return Err("unknown microphone test UI function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices() -> Value {
        json!([
            {"id":1,"kind":"output","name":"Speakers","node":"sink.speakers","default":true},
            {"id":2,"kind":"output","name":"Headphones","node":"sink.headphones"},
            {"id":3,"kind":"output","name":"HDMI card","node":""},
            {"id":4,"kind":"input","name":"Webcam","node":"source.webcam","default":true},
        ])
    }
    fn view(status: Value, meter: Value) -> Value {
        call("view", &[status, meter]).unwrap()
    }

    #[test]
    fn the_test_output_defaults_to_the_system_output_and_keeps_an_explicit_choice() {
        let idle = json!({"mode":"idle"});
        let resolved = call("devices", &[devices(), json!(""), idle.clone()]).unwrap();
        assert_eq!(resolved["output"], "sink.speakers");
        assert_eq!(resolved["outputName"], "Speakers");
        assert_eq!(resolved["input"], "source.webcam");
        assert_eq!(resolved["inputName"], "Webcam");
        // An output without a node cannot carry one stream and is not offered.
        assert_eq!(resolved["options"].as_array().unwrap().len(), 2);
        let chosen = call("devices", &[devices(), json!("sink.headphones"), idle.clone()]).unwrap();
        assert_eq!(chosen["output"], "sink.headphones");
        // A chosen output that is gone falls back only while nothing is running.
        let gone = call("devices", &[devices(), json!("sink.dock"), idle]).unwrap();
        assert_eq!(gone["output"], "sink.speakers");
        assert_eq!(gone["lost"], "");
    }

    #[test]
    fn a_device_that_disappears_under_a_running_test_ends_it_instead_of_rerouting() {
        let live = json!({"mode":"live","input":"source.webcam","output":"sink.headphones"});
        let intact = call("devices", &[devices(), json!("sink.headphones"), live.clone()]).unwrap();
        assert_eq!(intact["lost"], "");
        let without_output = json!([
            {"id":4,"kind":"input","name":"Webcam","node":"source.webcam","default":true},
        ]);
        let lost = call(
            "devices",
            &[without_output, json!("sink.headphones"), live.clone()],
        )
        .unwrap();
        assert_eq!(lost["lost"], "output");
        assert!(lost["message"].as_str().unwrap().contains("no longer"));
        let without_input = json!([
            {"id":2,"kind":"output","name":"Headphones","node":"sink.headphones"},
        ]);
        let lost = call("devices", &[without_input, json!("sink.headphones"), live]).unwrap();
        assert_eq!(lost["lost"], "input");
    }

    #[test]
    fn microphone_use_is_described_rather_than_claimed_and_never_invented() {
        let unavailable = call("users", &[json!([]), json!(false)]).unwrap();
        assert_eq!(unavailable["known"], false);
        assert_eq!(unavailable["busy"], false);
        assert!(unavailable["title"].as_str().unwrap().contains("cannot"));
        let quiet = call("users", &[json!([]), json!(true)]).unwrap();
        assert_eq!(quiet["busy"], false);
        assert_eq!(quiet["title"], "");
        assert_eq!(
            call("users", &[json!(["Firefox"]), json!(true)]).unwrap()["title"],
            "Firefox is using the microphone"
        );
        assert_eq!(
            call("users", &[json!(["Firefox", "Zoom"]), json!(true)]).unwrap()["title"],
            "Firefox and Zoom are using the microphone"
        );
        assert_eq!(
            call("users", &[json!(["a", "b", "c", "d"]), json!(true)]).unwrap()["title"],
            "a, b and 2 more are using the microphone"
        );
        // Nothing here says a call was detected.
        let busy = call("users", &[json!(["Zoom"]), json!(true)]).unwrap();
        for key in ["title", "detail"] {
            let text = busy[key].as_str().unwrap().to_lowercase();
            assert!(!text.contains("call"), "{key} claimed a call");
        }
    }

    #[test]
    fn both_modes_pass_the_same_gate_and_a_cancelled_warning_starts_nothing() {
        for action in ["sample", "live"] {
            let gated = call(
                "start",
                &[
                    json!(action),
                    json!("source.webcam"),
                    json!("sink.speakers"),
                    json!(["Zoom"]),
                    json!(true),
                    json!(false),
                ],
            )
            .unwrap();
            assert!(gated.get("command").is_none(), "{action} skipped the gate");
            assert_eq!(gated["confirm"]["busy"], true);
            assert_eq!(gated["action"], action);
            let confirmed = call(
                "start",
                &[
                    json!(action),
                    json!("source.webcam"),
                    json!("sink.speakers"),
                    json!(["Zoom"]),
                    json!(true),
                    json!(true),
                ],
            )
            .unwrap();
            assert_eq!(confirmed["command"]["command"], action);
            assert_eq!(confirmed["command"]["input"], "source.webcam");
            assert_eq!(confirmed["command"]["output"], "sink.speakers");
        }
        // An unavailable check reports its limitation; it does not become a
        // prompt no evidence could answer.
        let unknown = call(
            "start",
            &[
                json!("live"),
                json!("source.webcam"),
                json!("sink.speakers"),
                json!([]),
                json!(false),
                json!(false),
            ],
        )
        .unwrap();
        assert_eq!(unknown["command"]["command"], "live");
        assert!(call("start", &[json!("export")]).is_err());
    }

    #[test]
    fn a_test_without_a_resolved_device_reports_which_one_is_missing() {
        let no_input = call(
            "start",
            &[json!("sample"), json!(""), json!("sink.speakers")],
        )
        .unwrap();
        assert!(no_input["error"].as_str().unwrap().contains("microphone"));
        let no_output = call(
            "start",
            &[json!("live"), json!("source.webcam"), json!("")],
        )
        .unwrap();
        assert!(no_output["error"].as_str().unwrap().contains("output"));
    }

    #[test]
    fn the_meter_rests_when_nothing_is_being_captured() {
        let loud = json!({"level":0.9,"peak":0.99,"clipped":true,"remaining":2500});
        let recording = view(json!({"mode":"recording"}), loud.clone());
        assert_eq!(recording["level"], 0.9);
        assert_eq!(recording["clipped"], true);
        assert_eq!(recording["remaining"], "2.5 s left");
        assert_eq!(recording["detail"], "2.5 s left");
        // Playback is not capture: the recording's last frame is not replayed
        // as if the microphone were still delivering it.
        let playing = view(json!({"mode":"playing","sample":true}), loud.clone());
        assert_eq!(playing["level"], 0.0);
        assert_eq!(playing["clipped"], false);
        assert_eq!(playing["remaining"], "");
        let live = view(json!({"mode":"live"}), json!({"level":0.4,"peak":0.5}));
        assert_eq!(live["level"], 0.4);
        // A live monitor has no remaining time because it has no limit.
        assert_eq!(live["remaining"], "");
        assert_eq!(view(json!({}), json!({}))["level"], 0.0);
    }

    #[test]
    fn the_card_offers_only_what_the_current_state_allows() {
        let idle = view(json!({"mode":"idle"}), json!({}));
        assert_eq!(
            [
                idle["canRecord"].as_bool(),
                idle["canLive"].as_bool(),
                idle["canReplay"].as_bool(),
                idle["canStop"].as_bool()
            ],
            [Some(true), Some(true), Some(false), Some(false)]
        );
        let ready = view(json!({"mode":"idle","sample":true,"clipped":true}), json!({}));
        assert_eq!(ready["canReplay"], true);
        assert_eq!(ready["sampleClipped"], true);
        for mode in ["recording", "playing", "live"] {
            let active = view(json!({"mode":mode,"sample":true}), json!({}));
            assert_eq!(active["canStop"], true, "{mode} must stay stoppable");
            assert_eq!(active["canRecord"], false);
            assert_eq!(active["canLive"], false);
            assert_eq!(active["canReplay"], false);
            assert_eq!(active["tone"], mode);
        }
    }

    #[test]
    fn mute_and_failure_are_reported_instead_of_being_worked_around() {
        let muted = view(json!({"mode":"live","muted":true}), json!({}));
        assert_eq!(muted["muted"], true);
        // Nothing here proposes unmuting or changing a level.
        assert_eq!(muted["mode"], "live");
        assert_eq!(view(json!({"mode":"idle","muted":true}), json!({}))["muted"], false);
        let failed = view(
            json!({"mode":"idle","error":"The microphone stopped delivering audio"}),
            json!({}),
        );
        assert_eq!(failed["tone"], "error");
        assert_eq!(failed["detail"], "The microphone stopped delivering audio");
        assert_eq!(failed["active"], false);
        assert_eq!(failed["canStop"], false);
    }

    #[test]
    fn unknown_functions_and_oversized_snapshots_are_refused() {
        assert!(call("export", &[]).is_err());
        assert!(call("devices", &[json!(vec![Value::Null; 4097])]).is_err());
    }
}
