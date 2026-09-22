//! Compositor-owned destinations and guarded, silent window moves for Vicinae.
use crate::{control, vicinae::parsed, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MAX_PROJECTED_TARGETS: usize = 4096;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Destination {
    selector: String,
    name: String,
    id: Option<i64>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Window {
    address: String,
    pid: i64,
    started: u64,
    initial_class: String,
    initial_title_hash: String,
    workspace: i64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Move {
    window: Window,
    destination: Destination,
}
fn regular_workspace(id: i64, name: &str) -> bool {
    id != 0
        && id != -1
        && id <= i32::MAX as i64
        && !(-99..=-2).contains(&id)
        && !name.starts_with("special:")
        && name != "special"
}
fn selector(id: i64, name: &str) -> Option<String> {
    if !regular_workspace(id, name) {
        return None;
    }
    if id > 0 {
        Some(id.to_string())
    } else {
        exact_selector(&format!("name:{name}"))
    }
}
fn exact_selector(value: &str) -> Option<String> {
    if let Ok(id) = value.parse::<i32>() {
        return (id > 0 && id.to_string() == value).then(|| value.to_owned());
    }
    value
        .strip_prefix("name:")
        .filter(|name| !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control))
        .map(|_| value.to_owned())
}
pub(crate) fn workspace_rules() -> Value {
    parsed("hyprctl", &["workspacerules", "-j"])
        .ok()
        .filter(|rules| rules.as_array().is_some_and(|rows| rows.len() <= 4096))
        .unwrap_or_else(|| json!([]))
}
fn destinations(workspaces: &Value, rules: &Value) -> Result<Vec<Destination>> {
    let workspaces = workspaces.as_array().ok_or("Invalid workspace snapshot")?;
    let rules = rules.as_array().ok_or("Invalid workspace rules")?;
    if workspaces.len() > 4096 || rules.len() > 4096 {
        return Err("Too many workspaces".into());
    }
    let mut targets = BTreeMap::new();
    for row in workspaces {
        let (Some(id), Some(name)) = (row["id"].as_i64(), row["name"].as_str()) else {
            continue;
        };
        if let Some(selector) = selector(id, name) {
            targets.insert(
                selector.clone(),
                Destination {
                    selector,
                    name: name.to_owned(),
                    id: Some(id),
                },
            );
        }
    }
    for row in rules {
        if row["enabled"] == false {
            continue;
        }
        let Some(selector) = row["workspaceString"].as_str().and_then(exact_selector) else {
            continue;
        };
        // A named rule can refer to a renamed numbered workspace already listed.
        if targets
            .values()
            .any(|target| selector.strip_prefix("name:") == Some(target.name.as_str()))
        {
            continue;
        }
        targets
            .entry(selector.clone())
            .or_insert_with(|| Destination {
                name: selector
                    .strip_prefix("name:")
                    .unwrap_or(&selector)
                    .to_owned(),
                selector,
                id: None,
            });
    }
    let mut result = targets.into_values().collect::<Vec<_>>();
    result.sort_by(
        |a, b| match (a.selector.parse::<i64>(), b.selector.parse::<i64>()) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        },
    );
    Ok(result)
}
fn window(row: &Value) -> Option<Window> {
    let address = row["address"].as_str()?;
    if !address.starts_with("0x") || row["mapped"] != true || row["hidden"] == true {
        return None;
    }
    let address = format!("0x{}", control::window_address(address).ok()?);
    let pid = row["pid"]
        .as_i64()
        .filter(|pid| (2..=i32::MAX as i64).contains(pid))?;
    let started = crate::daemon::start_time(pid as u32)?;
    let workspace = row["workspace"]["id"].as_i64()?;
    if !regular_workspace(workspace, row["workspace"]["name"].as_str()?) {
        return None;
    }
    Some(Window {
        address,
        pid,
        started,
        workspace,
        initial_class: row["initialClass"].as_str()?.to_owned(),
        initial_title_hash: format!(
            "{:x}",
            Sha256::digest(row["initialTitle"].as_str()?.as_bytes())
        ),
    })
}
pub(crate) fn enrich(
    snapshot: &mut Value,
    clients: &Value,
    workspaces: &Value,
    rules: &Value,
) -> Result {
    let targets = destinations(workspaces, rules)?;
    let clients = clients.as_array().ok_or("Invalid windows")?;
    let identities = clients
        .iter()
        .filter_map(|row| window(row).map(|identity| (identity.address.clone(), identity)))
        .collect::<BTreeMap<_, _>>();
    let groups = snapshot["clientGroups"]
        .as_array()
        .ok_or("Invalid window groups")?;
    let client_count = groups.iter().try_fold(0usize, |count, group| {
        count
            .checked_add(group.as_array().ok_or("Invalid windows")?.len())
            .ok_or("Too many windows")
    })?;
    if client_count
        .checked_mul(targets.len())
        .is_none_or(|count| count > MAX_PROJECTED_TARGETS)
    {
        return Err("Too many window move targets".into());
    }
    for group in snapshot["clientGroups"]
        .as_array_mut()
        .ok_or("Invalid window groups")?
    {
        for client in group.as_array_mut().ok_or("Invalid windows")? {
            let identity = client["address"]
                .as_str()
                .and_then(|address| control::window_address(address).ok())
                .and_then(|address| identities.get(&format!("0x{address}")));
            let choices = identity
                .map(|identity| {
                    targets
                        .iter()
                        .filter(|target| {
                            target.id != Some(identity.workspace)
                                && target.selector != identity.workspace.to_string()
                                && target.selector.strip_prefix("name:")
                                    != client["workspace"]["name"].as_str()
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            client["moveWindow"] = json!(identity);
            client["moveTargets"] = json!(choices);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_is_bounded_before_targets_are_cloned() {
        let client = json!({
            "address": "0x1",
            "pid": std::process::id(),
            "mapped": true,
            "hidden": false,
            "initialClass": "fixture",
            "initialTitle": "Fixture",
            "workspace": {"id": 1, "name": "one"}
        });
        let clients = json!([client]);
        let mut snapshot = json!({
            "clientGroups": [(0..65).map(|_| json!({
                "address": "0x1",
                "workspace": {"name": "one"}
            })).collect::<Vec<_>>()]
        });
        let workspaces = json!((2..66)
            .map(|id| json!({
                "id": id,
                "name": id.to_string()
            }))
            .collect::<Vec<_>>());

        assert_eq!(
            enrich(&mut snapshot, &clients, &workspaces, &json!([]))
                .unwrap_err()
                .to_string(),
            "Too many window move targets"
        );
        assert!(snapshot["clientGroups"][0][0].get("moveTargets").is_none());
    }
}
fn focused() -> Result<(Value, Value)> {
    let active = parsed("hyprctl", &["activewindow", "-j"])?;
    let workspace = parsed("hyprctl", &["activeworkspace", "-j"])?;
    if !active.is_object() || workspace["id"].as_i64().is_none() {
        return Err("Focus unavailable".into());
    }
    Ok((
        json!([active["address"], active["pid"]]),
        workspace["id"].clone(),
    ))
}
pub(crate) fn run(arguments: &[String]) -> Result {
    if arguments.len() != 1 || arguments[0].len() > 65536 {
        return Err("Invalid window move".into());
    }
    let request: Move = serde_json::from_str(&arguments[0])?;
    if request.window.pid <= 0
        || !request.window.address.starts_with("0x")
        || control::window_address(&request.window.address).is_err()
        || exact_selector(&request.destination.selector).is_none()
    {
        return Err("Invalid window move".into());
    }
    let before = focused()?;
    let workspaces = parsed("hyprctl", &["workspaces", "-j"])?;
    let rules = workspace_rules();
    if !destinations(&workspaces, &rules)?.contains(&request.destination) {
        return Err("Workspace changed; refresh the desktop".into());
    }
    let clients = parsed("hyprctl", &["clients", "-j"])?;
    let current = clients
        .as_array()
        .ok_or("Invalid windows")?
        .iter()
        .find_map(|row| window(row).filter(|identity| identity == &request.window))
        .ok_or("Window changed; refresh the desktop")?;
    if request.destination.id == Some(current.workspace)
        || request.destination.selector == current.workspace.to_string()
    {
        return Err("Window is already on that workspace".into());
    }
    // JSON strings are valid Lua strings here once non-ASCII is emitted literally;
    // control characters are rejected by exact_selector above.
    let workspace = serde_json::to_string(&request.destination.selector)?;
    control::dispatch(&format!("hl.dsp.window.move({{ window = \"address:{}\", workspace = {workspace}, follow = false }})", current.address))?;
    let after_clients = parsed("hyprctl", &["clients", "-j"])?;
    let moved = after_clients
        .as_array()
        .ok_or("Invalid windows")?
        .iter()
        .any(|row| {
            let Some(mut identity) = window(row) else {
                return false;
            };
            let id = identity.workspace;
            identity.workspace = current.workspace;
            identity == current
                && (request.destination.id == Some(id)
                    || request.destination.selector == id.to_string()
                    || request.destination.selector.strip_prefix("name:")
                        == row["workspace"]["name"].as_str())
        });
    let after = focused()?;
    // Moving the active client away necessarily lets Hyprland focus its replacement.
    // Every other move must retain the active client, and all moves retain workspace.
    if !moved || before.1 != after.1 || (before.0[0] != current.address && before.0 != after.0) {
        return Err("Window move could not be confirmed; refresh the desktop".into());
    }
    Ok(())
}
