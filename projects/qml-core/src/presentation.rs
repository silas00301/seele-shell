//! Batched shell presentation policy. Qt retains themes, live objects and dates.
use crate::value::{array, fixed, number, number_text, string, text, truthy};
use regex::{Regex, RegexSet};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};
fn get<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}
fn free(limit: &Value) -> f64 {
    if limit.is_null() {
        100.0
    } else {
        let used = if truthy(limit.get("usedPercent")) {
            number(limit.get("usedPercent"))
        } else {
            0.0
        };
        let free = 100.0 - (used + 0.5).floor();
        if free.is_nan() { free } else { free.max(0.0) }
    }
}
fn severity(free: f64) -> &'static str {
    if free <= 15.0 {
        "red"
    } else if free <= 30.0 {
        "yellow"
    } else {
        "accent"
    }
}
fn tokens(count: f64) -> String {
    if count >= 1e9 {
        format!("{}B", fixed(count / 1e9, 1))
    } else if count >= 1e6 {
        format!("{}M", fixed(count / 1e6, 1))
    } else if count >= 1e3 {
        format!("{}K", fixed(count / 1e3, 0))
    } else {
        number_text((count + 0.5).floor())
    }
}
struct Provider<'a> {
    id: String,
    name: String,
    limit: Option<&'a Value>,
}
impl<'a> Provider<'a> {
    fn new(value: &'a Value) -> Self {
        let mut limit = None;
        for item in array(value.get("limits")) {
            if limit.is_none_or(|previous: &Value| {
                number(item.get("usedPercent")) > number(previous.get("usedPercent"))
            }) {
                limit = Some(item);
            }
        }
        Self {
            id: text(value.get("id")).to_lowercase(),
            name: text(value.get("name")).to_lowercase(),
            limit,
        }
    }
}
fn limit<'a>(providers: &[Provider<'a>], wanted: &str) -> Option<&'a Value> {
    let wanted = wanted.to_lowercase();
    let mut result = None;
    for provider in providers {
        if provider.id != wanted && !provider.name.contains(&wanted) {
            continue;
        }
        if let Some(value) = provider.limit.filter(|value| {
            result.is_none_or(|previous: &Value| {
                number(value.get("usedPercent")) > number(previous.get("usedPercent"))
            })
        }) {
            result = Some(value);
        }
    }
    result
}
fn agents(args: &[Value]) -> Result<Value, String> {
    let launchers = array(args.first());
    let subscriptions = array(args.get(1));
    let states = array(args.get(2));
    if launchers.len() > 4096 || subscriptions.len() > 1024 || states.len() > 4096 {
        return Err("agent presentation list exceeds its limit".into());
    }
    let mut names: HashMap<String, String> = [
        ("pi", "Pi"),
        ("opencode", "OpenCode"),
        ("codex", "Codex"),
        ("claude", "Claude Code"),
    ]
    .into_iter()
    .map(|(a, b)| (a.into(), b.into()))
    .collect();
    let mut ids: Vec<String> = ["pi", "opencode", "codex", "claude"]
        .into_iter()
        .map(String::from)
        .collect();
    let mut known_ids: HashSet<String> = ids.iter().cloned().collect();
    let mut indicators = Vec::new();
    let mut seen = HashSet::new();
    let mut state_map = HashMap::new();
    let (mut working, mut waiting, mut finished) = (0, 0, 0);
    for launcher in launchers {
        let id = text(launcher.get("id"));
        names.insert(id.clone(), text(launcher.get("name")));
        indicators.push(json!({"id":launcher["id"],"name":launcher["name"]}));
        seen.insert(id);
    }
    for item in states {
        let id = get(item, "id");
        let state = &item["state"];
        state_map.insert(id, state);
        if known_ids.insert(id.into()) {
            ids.push(id.into());
        }
        if truthy(state.get("active")) && !seen.contains(id) {
            indicators.push(json!({"id":id,"name":id}));
        }
        match get(state, "status") {
            "working" => working += 1,
            "input" => waiting += 1,
            "finished" => finished += 1,
            _ => {}
        }
    }
    let active:Vec<_>=ids.iter().filter_map(|id|state_map.get(id.as_str()).filter(|state|truthy(state.get("active"))).map(|state|json!({"id":id,"name":names.get(id).filter(|name|!name.is_empty()).unwrap_or(id),"status":if truthy(state.get("status")){string(state.get("status"))}else{"running".into()}}))).collect();
    let mut parts = Vec::new();
    if working > 0 {
        parts.push(format!("{working} working"));
    }
    if waiting > 0 {
        parts.push(format!(
            "{waiting} {} input",
            if waiting == 1 { "needs" } else { "need" }
        ));
    }
    if finished > 0 {
        parts.push(format!("{finished} finished"));
    }
    let mut capacities = Vec::new();
    let mut capacity_parts = Vec::new();
    let mut codex = None;
    let mut provider_names = Vec::new();
    let providers: Vec<_> = subscriptions.iter().map(Provider::new).collect();
    for subscription in subscriptions {
        let id = string(subscription.get("id"));
        let name = string(subscription.get("name"));
        provider_names.push(name.clone());
        let limit = limit(&providers, &id);
        let remaining = limit.map_or(100.0, free);
        if limit.is_some() && remaining < 100.0 {
            capacities.push(
                json!({"id":subscription["id"],"name":subscription["name"],"free":remaining}),
            );
            capacity_parts.push(format!("{name} {}% free", number_text(remaining)));
        }
        let identity = format!(
            "{} {}",
            text(subscription.get("id")),
            text(subscription.get("name"))
        )
        .to_lowercase();
        if codex.is_none() && (identity.contains("codex") || identity.contains("openai")) {
            let name = if truthy(subscription.get("name")) {
                name
            } else {
                "Codex".into()
            };
            codex = Some(format!(
                "{name}{}",
                if limit.is_some() {
                    format!(" · {}% free", number_text(remaining))
                } else {
                    String::new()
                }
            ));
        }
    }
    let codex = codex.unwrap_or_else(|| {
        if truthy(args.get(3)) {
            "Codex · checking usage…"
        } else if truthy(args.get(4)) {
            "Codex · usage unavailable"
        } else {
            "Codex · starts when you send"
        }
        .into()
    });
    Ok(
        json!({"active":active,"indicators":indicators,"statuses":states.iter().map(|item|(get(item,"id").to_owned(),json!(if truthy(item["state"].get("status")){string(item["state"].get("status"))}else{"idle".into()}))).collect::<Map<_,_>>(),"running":states.iter().map(|item|(get(item,"id").to_owned(),json!(truthy(item["state"].get("active"))))).collect::<Map<_,_>>(),"summary":parts.join(" · "),"subscriptions":if subscriptions.is_empty(){"No subscriptions".into()}else{provider_names.join(" · ")},"capacities":capacities,"capacityTip":if capacity_parts.is_empty(){"AI cockpit".into()}else{capacity_parts.join(" · ")},"codex":codex}),
    )
}
fn bluetooth_icon(device: &Value) -> &'static str {
    static PATTERNS: OnceLock<RegexSet> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        RegexSet::new([
            "airpod|buds|headphone|headset|beats|wh-|wf-",
            "speaker|soundcore|boom|jbl|sonos",
            "keyboard|keychron|k[0-9]+ ",
            "mouse|mx master",
            "controller|gamepad|dualsense|xbox",
            "phone|pixel|galaxy|iphone",
            "macbook|thinkpad|laptop",
            r"\[tv\]|fernseher|television",
            "watch|band",
        ])
        .expect("fixed Bluetooth labels")
    });
    let matches = patterns.matches(&text(device.get("name")).to_lowercase());
    let icon = text(device.get("icon"));
    if icon.contains("headset") || icon.contains("headphone") || matches.matched(0) {
        "󰋋"
    } else if icon.contains("speaker") || icon == "audio-card" || matches.matched(1) {
        "󰓃"
    } else if icon == "input-keyboard" || matches.matched(2) {
        "󰌌"
    } else if icon == "input-mouse" || matches.matched(3) {
        "󰍽"
    } else if icon == "input-gaming" || matches.matched(4) {
        "󰊴"
    } else if icon == "phone" || matches.matched(5) {
        "󰄜"
    } else if icon == "computer" || matches.matched(6) {
        "󰌢"
    } else if icon == "video-display" || matches.matched(7) {
        "󰔂"
    } else if icon == "printer" {
        "󰐪"
    } else if matches.matched(8) {
        "󰖐"
    } else {
        "󰂱"
    }
}
fn bluetooth_label(device: &Value, forget: &str, busy: &str, action: &str) -> Value {
    let connected = truthy(device.get("connected"));
    let paired = truthy(device.get("paired"));
    let address = device.get("address").and_then(Value::as_str);
    let suffix = if truthy(device.get("trusted")) {
        " · auto"
    } else {
        ""
    };
    let detail = if device.is_null() {
        String::new()
    } else if address == Some(forget) {
        "Tap again to forget".into()
    } else if address == Some(busy) {
        match action {
            "trust" => "Updating autoconnect…",
            "forget" => "Forgetting…",
            _ => {
                if connected {
                    "Disconnecting…"
                } else if paired {
                    "Connecting…"
                } else {
                    "Pairing…"
                }
            }
        }
        .into()
    } else if truthy(device.get("streaming")) {
        format!("Streaming here{suffix}")
    } else if connected {
        format!("Connected{suffix}")
    } else if paired {
        format!("Paired{suffix}")
    } else {
        "Available".into()
    };
    let signal = if device.is_null() || connected {
        String::new()
    } else if device.get("battery").is_some_and(|value| !value.is_null()) {
        format!("{}%", string(device.get("battery")))
    } else if !device.get("rssi").is_some_and(|value| !value.is_null()) {
        String::new()
    } else {
        let rssi = number(device.get("rssi"));
        if rssi >= -60.0 {
            "󰤨"
        } else if rssi >= -75.0 {
            "󰤥"
        } else {
            "󰤟"
        }
        .into()
    };
    json!({"icon":bluetooth_icon(device),"detail":detail,"signal":signal})
}
fn battery(entry: &Value) -> Value {
    let charging = text(entry.get("status")).to_lowercase() == "charging";
    let percent = if truthy(entry.get("percent")) {
        number(entry.get("percent"))
    } else {
        0.0
    };
    let icon = if entry.is_null() {
        "󰂑"
    } else if charging {
        "󰂄"
    } else if percent >= 80.0 {
        "󰁹"
    } else if percent >= 55.0 {
        "󰂀"
    } else if percent >= 30.0 {
        "󰁾"
    } else if percent >= 15.0 {
        "󰁻"
    } else {
        "󰂃"
    };
    let color = if charging {
        "green"
    } else if percent <= 15.0 {
        "red"
    } else if percent <= 30.0 {
        "yellow"
    } else {
        "text"
    };
    json!({"charging":charging,"icon":icon,"color":color})
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let first = args.first().unwrap_or(&null);
    Ok(match function {
        "agents" => return agents(args),
        "tokens" => json!(tokens(number(args.first()))),
        "free" => json!(free(first)),
        "capacityColor" => json!(severity(number(args.first()))),
        "reset" => {
            if first.is_null() {
                json!("")
            } else {
                let delta = number(args.first()) - number(args.get(1));
                let minutes = (delta / 60000.0).floor();
                let hours = (minutes / 60.0).floor();
                let days = (hours / 24.0).floor();
                json!(if delta.is_nan() || delta <= 0.0 {
                    "now".into()
                } else if days > 0.0 {
                    format!("{}d {}h", number_text(days), number_text(hours % 24.0))
                } else if hours > 0.0 {
                    format!("{}h {}m", number_text(hours), number_text(minutes % 60.0))
                } else {
                    format!("{}m", number_text(minutes.max(1.0)))
                })
            }
        }
        "ago" => {
            let seconds = ((number(args.get(1)) / 1000.0).floor() - number(args.first())).max(0.0);
            let minutes = (seconds / 60.0).floor();
            let hours = (minutes / 60.0).floor();
            json!(if seconds < 60.0 {
                "just now".into()
            } else if minutes < 60.0 {
                format!("{}m ago", number_text(minutes))
            } else if hours < 24.0 {
                format!("{}h ago", number_text(hours))
            } else {
                format!("{}d ago", number_text((hours / 24.0).floor()))
            })
        }
        "agentBadge" => {
            let id = string(args.first());
            json!(match id.as_str() {
                "pi" => "PI".into(),
                "opencode" => "OC".into(),
                "codex" => "CX".into(),
                "claude" => "CC".into(),
                _ => String::from_utf16_lossy(&id.encode_utf16().take(2).collect::<Vec<_>>())
                    .to_uppercase(),
            })
        }
        "agentMark" => json!(match string(args.first()).to_lowercase().as_str() {
            "pi" => "pi.svg",
            "opencode" => "opencode.svg",
            "codex" | "openai" => "openai.svg",
            "claude" => "claude.svg",
            _ => "",
        }),
        "agentColor" => json!(match first.as_str() {
            Some("input") => "yellow",
            Some("working") => "accent",
            Some("finished") => "green",
            _ => "subtext",
        }),
        "agentStatusText" => json!(match first.as_str() {
            Some("input") => "needs input".into(),
            Some("idle") => "not running".into(),
            _ => string(args.first()),
        }),
        "bluetoothLabel" => bluetooth_label(
            first,
            &text(args.get(1)),
            &text(args.get(2)),
            &text(args.get(3)),
        ),
        "bluetooth" => {
            let devices = array(args.first());
            if devices.len() > 4096 {
                return Err("Bluetooth list exceeds its limit".into());
            }
            let mut labels = Map::new();
            let (mut connected, mut streaming) = (0, 0);
            for device in devices {
                labels.insert(
                    get(device, "address").into(),
                    bluetooth_label(
                        device,
                        &text(args.get(2)),
                        &text(args.get(3)),
                        &text(args.get(4)),
                    ),
                );
                if truthy(device.get("source")) && truthy(device.get("connected")) {
                    connected += 1;
                    if truthy(device.get("streaming")) {
                        streaming += 1;
                    }
                }
            }
            let detail = if streaming > 0 {
                format!(
                    "{streaming} device{} streaming",
                    if streaming == 1 { "" } else { "s" }
                )
            } else if connected > 0 {
                format!(
                    "{connected} device{} connected",
                    if connected == 1 { "" } else { "s" }
                )
            } else if !truthy(args.get(1)) {
                "Play a phone through this PC".into()
            } else {
                "Waiting for a paired device".into()
            };
            json!({"labels":labels,"receiver":detail})
        }
        "battery" => battery(first),
        "batteries" => {
            let entries = array(args.first());
            if entries.len() > 4096 {
                return Err("battery list exceeds its limit".into());
            }
            let headphones = args.get(1).unwrap_or(&null);
            let mut labels = Map::new();
            let (mut primary, mut lowest) = (None, None);
            let mut values = Vec::new();
            static AIRPODS: OnceLock<Regex> = OnceLock::new();
            let airpods = AIRPODS.get_or_init(|| {
                Regex::new(&format!("(?i)^AirPods{}*", crate::value::SPACE))
                    .expect("fixed AirPods label")
            });
            for (index, entry) in entries.iter().enumerate() {
                labels.insert(get(entry, "name").into(), battery(entry));
                if get(entry, "kind") == "system" && primary.is_none() {
                    primary = Some(index);
                }
                if lowest.is_none_or(|old: usize| {
                    number(entry.get("percent")) < number(entries[old].get("percent"))
                }) {
                    lowest = Some(index);
                }
                let name = text(entry.get("name"));
                if name.to_lowercase().contains("airpods") {
                    let component = airpods.replace(&name, "");
                    values.push(format!(
                        "{} {}%",
                        if component.is_empty() {
                            "battery"
                        } else {
                            &component
                        },
                        number_text(number(entry.get("percent")))
                    ));
                }
            }
            let battery = if get(headphones, "kind") == "nothing"
                && headphones
                    .get("battery")
                    .is_some_and(|value| !value.is_null())
            {
                format!("{}%", number_text(number(headphones.get("battery"))))
            } else {
                values.join(" · ")
            };
            let connected = truthy(headphones.get("connected"));
            let name = text(headphones.get("name"));
            json!({"labels":labels,"primary":primary.or(lowest),"headphonesBattery":battery,"headphonesKind":if connected&&name.to_lowercase().contains("airpods"){"airpods"}else{"headphones"},"headphonesLabel":if connected&&!name.is_empty(){name}else{"Headphones".into()},"headphonesDetail":if !connected{"Not connected".into()}else if battery.is_empty(){"Connected".into()}else{battery}})
        }
        _ => return Err("unknown shell presentation function".into()),
    })
}
