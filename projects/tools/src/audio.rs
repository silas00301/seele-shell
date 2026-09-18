use serde_json::{json, Value};
use std::collections::HashSet;

// The panel can only draw what fits behind its own scroll viewport, and the
// graph is the one status source a hostile or broken application can grow by
// itself. Publish at most this many application rows.
pub const MAX_STREAMS: usize = 16;
// A name arrives from another process, so it is bounded before it reaches the
// wire rather than after it reaches a delegate.
const MAX_NAME: usize = 128;

fn contains_sink(value: &Value) -> bool {
    match value {
        Value::String(value) => value == "Audio/Sink",
        Value::Array(values) => values.iter().any(contains_sink),
        Value::Object(values) => values.values().any(contains_sink),
        _ => false,
    }
}

pub fn devices(dump: &Value) -> Vec<Value> {
    let mut default_sink = String::new();
    let mut default_source = String::new();
    for object in dump.as_array().into_iter().flatten().filter(|object| {
        object.get("type").and_then(Value::as_str) == Some("PipeWire:Interface:Metadata")
            && object
                .pointer("/props/metadata.name")
                .and_then(Value::as_str)
                == Some("default")
    }) {
        for item in object
            .get("metadata")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let key = item["key"].as_str().unwrap_or("");
            let name = item
                .pointer("/value/name")
                .and_then(Value::as_str)
                .unwrap_or("");
            if key == "default.audio.sink" {
                default_sink = name.into()
            }
            if key == "default.audio.source" {
                default_source = name.into()
            }
        }
    }
    let combined: Vec<&str> = dump
        .as_array()
        .into_iter()
        .flatten()
        .find(|object| {
            object
                .pointer("/info/props/node.name")
                .and_then(Value::as_str)
                == Some(default_sink.as_str())
                && crate::audio_route::NAMES.contains(&default_sink.as_str())
        })
        .and_then(|object| {
            object
                .pointer("/info/props/seele.outputs")
                .and_then(Value::as_str)
        })
        .unwrap_or("")
        .split(',')
        .filter(|name| !name.is_empty())
        .collect();
    let mut values = Vec::new();
    for object in dump.as_array().into_iter().flatten() {
        let class = object
            .pointer("/info/props/media.class")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !matches!(class, "Audio/Sink" | "Audio/Source") {
            continue;
        }
        let props = object.pointer("/info/props").unwrap_or(&Value::Null);
        let node = props["node.name"].as_str().unwrap_or("");
        if crate::audio_route::NAMES.contains(&node) {
            continue;
        }
        let kind = if class == "Audio/Sink" {
            "output"
        } else {
            "input"
        };
        values.push(json!({"id":object["id"],"kind":kind,"name":props["node.description"].as_str().or_else(||props["node.nick"].as_str()).or_else(||props["node.name"].as_str()).unwrap_or(""),"node":node,"profile":Value::Null,"selected":if kind=="output"{node==default_sink || combined.contains(&node)}else{node==default_source},"default":if kind=="output"{node==default_sink}else{node==default_source}}));
    }
    let sink_devices: HashSet<u64> = dump
        .as_array()
        .into_iter()
        .flatten()
        .filter(|object| {
            object
                .pointer("/info/props/media.class")
                .and_then(Value::as_str)
                == Some("Audio/Sink")
        })
        .filter_map(|object| {
            object
                .pointer("/info/props/device.id")
                .and_then(Value::as_u64)
        })
        .collect();
    for device in dump.as_array().into_iter().flatten() {
        if device["type"].as_str() != Some("PipeWire:Interface:Device")
            || device
                .pointer("/info/props/media.class")
                .and_then(Value::as_str)
                != Some("Audio/Device")
            || device["id"]
                .as_u64()
                .is_some_and(|id| sink_devices.contains(&id))
        {
            continue;
        }
        let props = &device["info"]["props"];
        let name = props["device.description"]
            .as_str()
            .or_else(|| props["device.name"].as_str())
            .unwrap_or("");
        for profile in device
            .pointer("/info/params/EnumProfile")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if profile["available"].as_str() != Some("yes")
                || !contains_sink(&profile["classes"])
                || profile["index"].as_u64().is_none()
            {
                continue;
            }
            let description = profile["description"].as_str().unwrap_or("");
            values.push(json!({"id":device["id"],"kind":"output","name":format!("{name} · {description}"),"node":"","profile":profile["index"],"default":false}));
        }
    }
    values.sort_by_cached_key(|value| {
        (
            value["kind"].as_str().unwrap_or("").to_owned(),
            value["name"].as_str().unwrap_or("").to_ascii_lowercase(),
        )
    });
    values
}

fn bounded(value: &str) -> String {
    value.trim().chars().take(MAX_NAME).collect()
}

// An application names itself in several places and agrees with itself in
// none of them. Take the first thing that names the *program*: the track
// title is the last resort rather than the first, because a row titled with
// whatever is playing renames itself between songs and stops being the thing
// the pointer was aimed at.
fn stream_name(props: &Value) -> String {
    for key in [
        "application.name",
        "node.description",
        "application.process.binary",
        "node.name",
        "media.name",
    ] {
        let value = bounded(props[key].as_str().unwrap_or(""));
        if !value.is_empty() {
            return value;
        }
    }
    // Nothing here identifies a program. Saying so is more useful than
    // borrowing the node id, which names nothing the user can act on.
    "Unnamed stream".into()
}

// PipeWire keeps a node's gain in the graph in linear scale. wpctl reads and
// writes the cube root of it, because it sets WirePlumber's mixer-api to the
// cubic scale before running any command, and the master rows in this panel
// are exactly what wpctl reports. Convert in the same direction here, once, or
// an application at half volume would sit at 13% beside a master at 50%.
// Amplification above full stays visible for the same reason the media panel
// shows an amplified player honestly: the shell never writes it, but something
// else may have.
fn stream_percent(node: &Value) -> u64 {
    let props = &node["info"]["params"]["Props"][0];
    let linear = props
        .pointer("/channelVolumes/0")
        .and_then(Value::as_f64)
        .or_else(|| props["volume"].as_f64())
        .unwrap_or(0.0);
    if !linear.is_finite() || linear <= 0.0 {
        return 0;
    }
    (linear.cbrt() * 100.0).round().min(1000.0) as u64
}

// Combining outputs loads one PipeWire stream per member sink, and those
// streams carry the combined sink's own name rather than an application's.
// They are the master output arriving twice, not two applications, so they
// never reach the mixer. The naming is upstream's: module-combine-stream
// builds `output.<combined node name>_<member>` for every member it feeds.
fn combined_output_stream(node: &str) -> bool {
    node.strip_prefix("output.").is_some_and(|rest| {
        crate::audio_route::NAMES
            .iter()
            .any(|name| rest.starts_with(&format!("{name}_")))
    })
}

fn stream_key(stream: &Value) -> (String, u64) {
    (
        stream["name"].as_str().unwrap_or("").to_ascii_lowercase(),
        stream["id"].as_u64().unwrap_or_default(),
    )
}

/// Every playback stream in the graph an application could own, named, levelled
/// and ordered. Ordering is by name and then by registry id: a row's place must
/// depend only on what the user can change, because a list that reorders itself
/// when a stream starts or stops moves a row out from under the pointer aimed
/// at it.
pub fn streams(dump: &Value) -> Vec<Value> {
    let mut values = Vec::new();
    for object in dump.as_array().into_iter().flatten() {
        let props = object.pointer("/info/props").unwrap_or(&Value::Null);
        if props["media.class"].as_str() != Some("Stream/Output/Audio") {
            continue;
        }
        // A system ding is a stream that exists for half a second, and it says
        // so itself. Excluding it on the role it declares is exact, costs
        // nothing, and needs no clock; excluding it on how long it has lasted
        // would delay every real stream to catch a class of stream that has
        // already identified itself.
        if matches!(
            props["media.role"]
                .as_str()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "event" | "notification"
        ) {
            continue;
        }
        if combined_output_stream(props["node.name"].as_str().unwrap_or("")) {
            continue;
        }
        let name = stream_name(props);
        // The track or page title identifies which of two windows of the same
        // application is the loud one. It belongs in the row's tooltip rather
        // than on the row, and only where it says something the title does not.
        let detail = bounded(props["media.name"].as_str().unwrap_or(""));
        values.push(json!({
            "id": object["id"],
            "name": name,
            "detail": if detail == name { String::new() } else { detail },
            "icon": bounded(props["application.icon-name"].as_str().unwrap_or("")),
            "volume": stream_percent(object),
            "muted": object.pointer("/info/params/Props/0/mute").and_then(Value::as_bool).unwrap_or(false),
            "playing": object.pointer("/info/state").and_then(Value::as_str) == Some("running"),
        }));
    }
    values.sort_by_cached_key(stream_key);
    values
}

/// Which playback streams the Audio panel is currently willing to show.
///
/// A stream joins the mixer the first time it is seen actually running, and
/// keeps its row until its node leaves the graph. That is what a debounce was
/// wanted for, without a clock: an application that has never played does not
/// appear merely because it opened an audio device, a stream that stops
/// between two tracks keeps the row the pointer is already on, and every
/// transition is a PipeWire event rather than a timer the monitor has no
/// reason to wake for.
#[derive(Default)]
pub struct StreamGate {
    admitted: HashSet<u64>,
}

impl StreamGate {
    pub fn admit(&mut self, streams: Vec<Value>) -> Vec<Value> {
        let present: HashSet<u64> = streams
            .iter()
            .filter_map(|stream| stream["id"].as_u64())
            .collect();
        self.admitted.retain(|id| present.contains(id));
        self.admitted.extend(
            streams
                .iter()
                .filter(|stream| stream["playing"].as_bool() == Some(true))
                .filter_map(|stream| stream["id"].as_u64()),
        );
        let mut values: Vec<Value> = streams
            .into_iter()
            .filter(|stream| {
                stream["id"]
                    .as_u64()
                    .is_some_and(|id| self.admitted.contains(&id))
            })
            .collect();
        if values.len() > MAX_STREAMS {
            // The bound exists so the panel cannot grow without limit, and what
            // it must not spend itself on is a stream that has fallen silent:
            // a quiet row yields its place to one that is actually playing.
            let (mut playing, idle): (Vec<Value>, Vec<Value>) = values
                .into_iter()
                .partition(|stream| stream["playing"].as_bool() == Some(true));
            playing.truncate(MAX_STREAMS);
            playing.extend(idle.into_iter().take(MAX_STREAMS - playing.len()));
            playing.sort_by_cached_key(stream_key);
            values = playing;
        }
        values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: u64) -> Value {
        json!({"id":id,"type":"PipeWire:Interface:Device","info":{
        "props":{"media.class":"Audio/Device","device.description":"Built-in Audio"},
        "params":{"EnumProfile":[
            {"index":0,"description":"Off","available":"yes","classes":[]},
            {"index":1,"description":"Analog Stereo","available":"yes","classes":[["Audio/Sink",1]]},
            {"index":2,"description":"Unplugged HDMI","available":"no","classes":[["Audio/Sink",1]]},
            {"index":3,"description":"Pro Audio","available":"unknown","classes":[["Audio/Sink",1]]},
            {"index":4,"description":"Input","available":"yes","classes":[["Audio/Source",1]]}
        ]}}})
    }

    #[test]
    fn combined_output_selects_physical_members_without_showing_virtual_sink() {
        let sink = |id, name| {
            json!({"id":id,"type":"PipeWire:Interface:Node","info":{"props":{
                "media.class":"Audio/Sink","node.name":name,"node.description":name
            }}})
        };
        let mut combined = sink(30, "seele_outputs_a");
        combined["info"]["props"]["seele.outputs"] = json!("speakers,headphones");
        let metadata = json!({"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
            "metadata":[{"key":"default.audio.sink","value":{"name":"seele_outputs_a"}}]});
        let entries = devices(&json!([
            sink(1, "speakers"),
            sink(2, "headphones"),
            sink(3, "hdmi"),
            combined,
            metadata
        ]));
        assert_eq!(entries.len(), 3);
        for entry in entries {
            assert_eq!(entry["selected"], entry["node"] != "hdmi");
            assert_eq!(entry["default"], false);
        }
    }

    #[test]
    fn inactive_card_offers_only_available_output_profiles() {
        let entries = devices(&json!([card(10)]));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["id"], 10);
        assert_eq!(entries[0]["profile"], 1);
        assert_eq!(entries[0]["default"], false);
    }

    #[test]
    fn active_sink_hides_profiles_and_retains_default_selection() {
        let sink = json!({"id":20,"type":"PipeWire:Interface:Node","info":{"props":{
            "media.class":"Audio/Sink","device.id":10,"node.name":"analog","node.description":"Speakers"
        }}});
        let metadata = json!({"type":"PipeWire:Interface:Metadata","props":{"metadata.name":"default"},
            "metadata":[{"key":"default.audio.sink","value":{"name":"analog"}}]});
        let entries = devices(&json!([card(10), sink, metadata]));
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["id"], 20);
        assert!(entries[0]["profile"].is_null());
        assert_eq!(entries[0]["default"], true);
    }

    fn stream(id: u64, mut props: Value) -> Value {
        props["media.class"] = json!("Stream/Output/Audio");
        json!({"id":id,"type":"PipeWire:Interface:Node","info":{
            "state":"running","props":props,
            "params":{"Props":[{"volume":1.0,"channelVolumes":[0.125,0.125],"mute":false}]}}})
    }

    #[test]
    fn a_stream_is_named_after_its_program_and_levelled_the_way_wpctl_reports() {
        let entries = streams(&json!([
            stream(10, json!({"application.name":"Zen Browser","media.name":"A song","application.icon-name":"zen"})),
            stream(11, json!({"node.description":"Bluetooth Receiver"})),
            stream(12, json!({"application.process.binary":"mpv","media.name":"mpv"})),
            stream(13, json!({})),
        ]));
        let names: Vec<_> = entries
            .iter()
            .map(|entry| entry["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            ["Bluetooth Receiver", "mpv", "Unnamed stream", "Zen Browser"]
        );
        let zen = entries.iter().find(|entry| entry["id"] == 10).unwrap();
        // 0.125 linear is half volume in the cubic scale wpctl speaks, and the
        // master rows beside this one are whatever wpctl reported.
        assert_eq!(zen["volume"], 50);
        assert_eq!(zen["detail"], "A song");
        assert_eq!(zen["icon"], "zen");
        assert_eq!(zen["playing"], true);
        // A title that only repeats the application's name is not a detail.
        let mpv = entries.iter().find(|entry| entry["id"] == 12).unwrap();
        assert_eq!(mpv["detail"], "");
        let amplified = streams(&json!([{"id":20,"info":{"state":"idle",
            "props":{"media.class":"Stream/Output/Audio","application.name":"Loud"},
            "params":{"Props":[{"channelVolumes":[3.375],"mute":true}]}}}]));
        assert_eq!(amplified[0]["volume"], 150);
        assert_eq!(amplified[0]["muted"], true);
        assert_eq!(amplified[0]["playing"], false);
    }

    #[test]
    fn system_sounds_and_the_shell_own_output_mirror_never_reach_the_mixer() {
        let entries = streams(&json!([
            stream(30, json!({"application.name":"Ding","media.role":"Event"})),
            stream(31, json!({"application.name":"Toast","media.role":"notification"})),
            stream(32, json!({"node.name":"output.seele_outputs_a_alsa_speakers"})),
            stream(33, json!({"application.name":"Kept","node.name":"output.other_sink"})),
            json!({"id":34,"info":{"state":"running","props":{"media.class":"Stream/Input/Audio","application.name":"Recorder"}}}),
        ]));
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, [33]);
    }

    #[test]
    fn a_stream_joins_when_it_plays_and_leaves_when_its_node_does() {
        let mut gate = StreamGate::default();
        let corked = json!([{"id":40,"name":"Zen","playing":false}]);
        assert!(gate.admit(corked.as_array().unwrap().clone()).is_empty());
        let playing = json!([{"id":40,"name":"Zen","playing":true}]);
        assert_eq!(gate.admit(playing.as_array().unwrap().clone()).len(), 1);
        // Paused between two tracks, the row stays where the pointer left it.
        assert_eq!(gate.admit(corked.as_array().unwrap().clone()).len(), 1);
        assert!(gate.admit(Vec::new()).is_empty());
        // The node is gone, so nothing remembers it back into the list.
        assert!(gate.admit(corked.as_array().unwrap().clone()).is_empty());
    }

    #[test]
    fn the_published_list_is_bounded_and_spends_itself_on_what_is_playing() {
        let mut gate = StreamGate::default();
        let mut values: Vec<Value> = (0..MAX_STREAMS as u64 + 4)
            .map(|index| json!({"id":index,"name":format!("app{index:02}"),"playing":true}))
            .collect();
        assert_eq!(gate.admit(values.clone()).len(), MAX_STREAMS);
        for value in values.iter_mut().take(6) {
            value["playing"] = json!(false);
        }
        let admitted = gate.admit(values);
        assert_eq!(admitted.len(), MAX_STREAMS);
        assert_eq!(
            admitted
                .iter()
                .filter(|entry| entry["playing"].as_bool() == Some(true))
                .count(),
            MAX_STREAMS - 2
        );
        // Bounding must not leave the rows out of order.
        let names: Vec<_> = admitted
            .iter()
            .map(|entry| entry["name"].as_str().unwrap().to_owned())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn equal_case_insensitive_names_keep_graph_order() {
        let sink = |id, name| {
            json!({"id":id,"info":{"props":{
                "media.class":"Audio/Sink","node.name":name,"node.description":name
            }}})
        };
        let entries = devices(&json!([
            sink(10, "Speakers"),
            sink(20, "headphones"),
            sink(30, "SPEAKERS")
        ]));
        let ids: Vec<_> = entries
            .iter()
            .map(|entry| entry["id"].as_u64().unwrap())
            .collect();
        assert_eq!(ids, [20, 10, 30]);
    }
}
