//! Session-local output mirroring through PipeWire's PulseAudio server.
use crate::command::{output, require_status};
use crate::Result;
use serde_json::Value;
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::time::Duration;

pub const NAMES: [&str; 2] = ["seele_outputs_a", "seele_outputs_b"];

fn list(kind: &str) -> Result<Vec<Value>> {
    let raw = output("pactl", ["--format=json", "list", kind]).ok_or("audio server unavailable")?;
    Ok(serde_json::from_str(&raw)?)
}

fn owned_name(module: &Value) -> Option<&'static str> {
    if module["name"] != "module-combine-sink" {
        return None;
    }
    NAMES.into_iter().find(|name| {
        module["argument"]
            .as_str()
            .unwrap_or("")
            .split_whitespace()
            .any(|arg| arg == format!("sink_name={name}"))
    })
}

fn numeric_id(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn validate(names: &[String], sinks: &[Value]) -> Result {
    if names.is_empty() {
        return Err("select at least one output".into());
    }
    let mut seen = std::collections::HashSet::new();
    for name in names {
        // Names enter the module's own argument parser after exec, so argv
        // separation alone is insufficient protection here.
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            || NAMES.contains(&name.as_str())
            || !seen.insert(name)
            || !sinks.iter().any(|sink| sink["name"] == *name)
        {
            return Err("output is unavailable or cannot be combined".into());
        }
    }
    Ok(())
}

pub fn set(names: &[String]) -> Result {
    let runtime =
        std::env::var_os("XDG_RUNTIME_DIR").ok_or("audio routing needs a user session")?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(std::path::PathBuf::from(runtime).join("seele-audio-outputs.lock"))?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("another output change is in progress".into());
    }
    let sinks = list("sinks")?;
    validate(names, &sinks)?;
    // pactl's module JSON omits its numeric index. The created sink carries
    // owner_module; verify ownership by module name and our reserved sink name
    // before using that ID. Stream owner IDs may be strings in the same API.
    let modules: Vec<_> = list("modules")?
        .iter()
        .filter_map(owned_name)
        .filter_map(|name| {
            sinks
                .iter()
                .find(|sink| sink["name"] == name)
                .and_then(|sink| numeric_id(&sink["owner_module"]))
                .map(|id| (name, id))
        })
        .collect();
    let previous = output("pactl", ["get-default-sink"]).ok_or("default output unavailable")?;
    let previous = previous.trim();
    let previous_index = sinks
        .iter()
        .find(|sink| sink["name"] == previous)
        .and_then(|sink| sink["index"].as_u64());
    let streams = list("sink-inputs")?;
    let target;
    let mut created = None;
    if names.len() == 1 {
        target = names[0].clone();
    } else {
        // Create the replacement first. A failed module load leaves the old
        // output and all its streams intact.
        target = NAMES
            .iter()
            .find(|name| !sinks.iter().any(|sink| sink["name"] == **name))
            .ok_or("both Seele output slots are occupied")?
            .to_string();
        let joined = names.join(",");
        let args = vec!["load-module".into(), "module-combine-sink".into(),
            format!("sink_name={target}"), format!("sinks={joined}"),
            "latency_compensate=true".into(),
            format!("sink_properties=\"device.description='Multiple Outputs' seele.outputs='{joined}'\"")];
        let id = output("pactl", &args)
            .ok_or("could not combine outputs")?
            .trim()
            .parse::<u64>()
            .map_err(|_| "invalid audio module id")?;
        created = Some(id);
    }
    let mut moved = Vec::new();
    let change = (|| -> Result {
        let mut ready = false;
        for _ in 0..50 {
            if list("sinks")?.iter().any(|sink| sink["name"] == target) {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        if !ready {
            return Err("combined output did not appear".into());
        }
        require_status("pactl", ["set-default-sink", target.as_str()])?;
        for stream in &streams {
            // Keep applications explicitly routed elsewhere on their output.
            if previous_index.is_some()
                && stream["sink"].as_u64() == previous_index
                && !modules
                    .iter()
                    .any(|(_, id)| Some(*id) == numeric_id(&stream["owner_module"]))
            {
                let id = stream["index"]
                    .as_u64()
                    .ok_or("invalid audio stream")?
                    .to_string();
                require_status("pactl", ["move-sink-input", id.as_str(), target.as_str()])?;
                moved.push(id);
            }
        }
        Ok(())
    })();
    if let Err(error) = change {
        let _ = output("pactl", ["set-default-sink", previous]);
        for id in moved {
            let _ = output("pactl", ["move-sink-input", id.as_str(), previous]);
        }
        if let Some(id) = created {
            let _ = output("pactl", ["unload-module", &id.to_string()]);
        }
        return Err(error);
    }
    let retired: Vec<_> = modules.iter().map(|(name, _)| *name).collect();
    for (_, id) in modules {
        require_status("pactl", ["unload-module", &id.to_string()])?;
    }
    // Pulse acknowledges unloading before PipeWire removes the node. Wait for
    // removal so an immediate second click can reuse the retired slot.
    for _ in 0..50 {
        if !list("sinks")?
            .iter()
            .any(|sink| retired.contains(&sink["name"].as_str().unwrap_or("")))
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("previous combined output did not retire".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn module_arguments_accept_only_available_unique_physical_names() {
        let sinks = vec![
            json!({"name":"alsa_output.pci-1"}),
            json!({"name":"bluez_output.AA_1"}),
        ];
        assert!(validate(
            &["alsa_output.pci-1".into(), "bluez_output.AA_1".into()],
            &sinks
        )
        .is_ok());
        for names in [
            vec![],
            vec!["missing"],
            vec!["alsa_output.pci-1", "alsa_output.pci-1"],
            vec!["alsa_output.pci-1 sinks=other"],
            vec!["seele_outputs_a"],
        ] {
            assert!(validate(
                &names.into_iter().map(String::from).collect::<Vec<_>>(),
                &sinks
            )
            .is_err());
        }
    }

    #[test]
    fn cleanup_only_owns_our_combine_modules() {
        assert_eq!(
            owned_name(
                &json!({"name":"module-combine-sink","argument":"sink_name=seele_outputs_a sinks=a,b"})
            ),
            Some("seele_outputs_a")
        );
        assert!(owned_name(
            &json!({"name":"module-combine-sink","argument":"sink_name=someone_else sinks=a,b"})
        )
        .is_none());
        assert!(owned_name(
            &json!({"name":"module-null-sink","argument":"sink_name=seele_outputs_a"})
        )
        .is_none());
    }
}
