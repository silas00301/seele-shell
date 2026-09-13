//! Native snapshot/action boundary for the launcher. Every subprocess is bounded
//! by the shared process owner; React receives display-ready metadata only.
use crate::{command, Result};
use seele_runtime::nix::{store_basename, store_name};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex,
};
use std::time::Duration;
const PROFILES: &str = "/nix/var/nix/profiles";
const RUNNING: &str = "/run/current-system";
const MAX_BYTES: usize = 4 * 1024 * 1024;
fn core(name: &str, args: &[Value]) -> Result<Value> {
    seele_qml_core::call(&format!("vicinae.{name}"), args).map_err(Into::into)
}
fn capture(program: &str, args: &[&str], timeout: Duration) -> Result<String> {
    command::output_with_input(program, args, b"", timeout, MAX_BYTES)
        .ok_or_else(|| "Desktop query failed".into())
}
fn parsed(program: &str, args: &[&str]) -> Result<Value> {
    Ok(serde_json::from_str(&capture(
        program,
        args,
        Duration::from_secs(15),
    )?)?)
}
fn canonical(path: &Path) -> Option<String> {
    fs::canonicalize(path)
        .ok()?
        .into_os_string()
        .into_string()
        .ok()
        .filter(|path| store_basename(path).is_some())
}
fn identities(
    rows: &[Value],
    resolve: impl Fn(&Path) -> Option<String> + Sync,
    cancel: &AtomicUsize,
) -> Result<Value> {
    if rows.len() > 4096 {
        return Err("Too many generations".into());
    }
    let next = AtomicUsize::new(0);
    let output = Mutex::new(BTreeMap::new());
    std::thread::scope(|scope| {
        for _ in 0..4.min(rows.len()) {
            let resolve = &resolve;
            let next = &next;
            let output = &output;
            scope.spawn(move || loop {
                if cancel.load(Ordering::Relaxed) != 0 {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(row) = rows.get(i) else {
                    break;
                };
                if let Some(path) = row["profilePath"]
                    .as_str()
                    .and_then(|path| resolve(Path::new(path)))
                    .filter(|path| store_basename(path).is_some())
                {
                    output
                        .lock()
                        .unwrap_or_else(|p| p.into_inner())
                        .insert(row["generation"].to_string(), path);
                }
            });
        }
    });
    if cancel.load(Ordering::Relaxed) != 0 {
        return Err("Generation lookup cancelled".into());
    }
    Ok(json!(output
        .into_inner()
        .unwrap_or_else(|p| p.into_inner())))
}
fn generations() -> Result<Value> {
    let payload = capture(
        "/run/current-system/sw/bin/nixos-rebuild",
        &["list-generations", "--json"],
        Duration::from_secs(15),
    )?;
    let rows = core("parseGenerations", &[json!(payload)])?;
    let running = canonical(Path::new(RUNNING)).ok_or("Running system is unavailable")?;
    let paths = identities(
        rows.as_array().ok_or("Invalid generation list")?,
        canonical,
        &command::shutdown_signal(),
    )?;
    core("markActiveGenerations", &[rows, json!(running), paths])
}
fn reviewed(arguments: &[String]) -> Result<(&str, &str, &str)> {
    if arguments.len() != 3 {
        return Err("Generation and both reviewed identities are required".into());
    }
    let number = &arguments[0];
    if number.is_empty()
        || number.starts_with('0')
        || number.len() > 16
        || !number.bytes().all(|b| b.is_ascii_digit())
        || number
            .parse::<u64>()
            .ok()
            .filter(|n| *n <= 9_007_199_254_740_991)
            .is_none()
        || !store_name(&arguments[1])
        || !store_name(&arguments[2])
    {
        return Err("Invalid reviewed generation".into());
    }
    Ok((number, &arguments[1], &arguments[2]))
}
fn check_review(arguments: &[String], profiles: &Path, running: &Path) -> Result {
    let (number, target, expected_running) = reviewed(arguments)?;
    let current = canonical(running).ok_or("Running system is unavailable")?;
    let selected = canonical(&profiles.join(format!("system-{number}-link")))
        .ok_or("Generation is no longer retained")?;
    if store_basename(&current) != Some(expected_running)
        || store_basename(&selected) != Some(target)
        || current == selected
    {
        return Err("Generation changed while the picker was open".into());
    }
    Ok(())
}
fn desktop() -> Result<Value> {
    let (clients, workspaces) = std::thread::scope(|scope| {
        let clients = scope.spawn(|| parsed("hyprctl", &["clients", "-j"]));
        let workspaces = parsed("hyprctl", &["workspaces", "-j"]);
        (
            clients
                .join()
                .unwrap_or_else(|_| Err("Window query failed".into())),
            workspaces,
        )
    });
    core("desktop", &[clients?, workspaces?])
}
fn focus(arguments: &[String]) -> Result {
    if arguments.len() != 2 {
        return Err("Desktop focus target is required".into());
    }
    let expression = match arguments[0].as_str() {
        "window" => {
            if !arguments[1].starts_with("0x") {
                return Err("Invalid window address".into());
            }
            let address = crate::control::window_address(&arguments[1])?;
            format!("hl.dsp.focus({{ window = \"address:0x{address}\" }})")
        }
        "workspace" => {
            let value = &arguments[1];
            let id = value
                .parse::<u64>()
                .ok()
                .filter(|id| *id > 0 && *id <= 9_007_199_254_740_991)
                .ok_or("Invalid workspace")?;
            if id.to_string() != *value {
                return Err("Invalid workspace".into());
            }
            format!("hl.dsp.focus({{ workspace = {id} }})")
        }
        _ => return Err("Invalid desktop focus operation".into()),
    };
    crate::control::dispatch(&expression)
}
pub fn run(arguments: &[String]) -> Result {
    let (operation, args) = arguments
        .split_first()
        .ok_or("Desktop operation required")?;
    match operation.as_str() {
        "vicinae-desktop" if args.is_empty() => println!("{}", desktop()?),
        "vicinae-keybindings" if args.is_empty() => println!(
            "{}",
            core("keybindings", &[parsed("hyprctl", &["binds", "-j"])?])?
        ),
        "vicinae-input-keybinding" if args.len() == 1 => {
            if args[0].len() > 4096 {
                return Err("Keybinding selection exceeds its limit".into());
            }
            let row: Value = serde_json::from_str(&args[0])?;
            let input = core("keybindingInput", &[row])?;
            let input = input
                .as_array()
                .ok_or("Invalid keybinding input")?
                .iter()
                .map(|v| v.as_str().ok_or("Invalid keybinding argument"))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if !input.is_empty() {
                capture("wtype", &input, Duration::from_secs(5))?;
            }
        }
        "vicinae-generations" if args.is_empty() => println!("{}", generations()?),
        "vicinae-generation-check" => {
            check_review(args, Path::new(PROFILES), Path::new(RUNNING))?;
            println!("{{\"ok\":true}}");
        }
        "vicinae-generation-diff" => {
            let (_, target, running) = reviewed(args)?;
            let target = format!("/nix/store/{target}");
            let running = format!("/nix/store/{running}");
            let output = capture("nvd", &["diff", &running, &target], Duration::from_secs(60))?;
            println!(
                "{}",
                json!({"diff":core("formatPackageDiff",&[json!(output)])?})
            );
        }
        "vicinae-focus" => focus(args)?,
        "vicinae-audio" if args.len() == 2 && matches!(args[1].as_str(), "select" | "toggle") => {
            if args[0].len() > 16384 {
                return Err("Audio selection exceeds its limit".into());
            }
            let wanted: Value = serde_json::from_str(&args[0])?;
            let dump = parsed("pw-dump", &[])?;
            let devices = crate::audio::devices(&dump);
            let selected = core(
                "audioSelection",
                &[wanted, json!(devices), json!(args[1] == "toggle")],
            )?;
            let selected = selected
                .as_array()
                .ok_or("Invalid audio selection")?
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(str::to_owned)
                        .ok_or("Invalid audio argument")
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if !selected.is_empty() {
                crate::control::run(&selected)?;
            }
        }
        _ => return Err("Invalid desktop operation".into()),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filesystem_resolution_has_a_fixed_worker_bound() {
        let rows = (1..=100)
            .map(|n| json!({"generation":n,"profilePath":format!("/fixture/{n}")}))
            .collect::<Vec<_>>();
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let cancel = AtomicUsize::new(0);
        let paths = identities(
            &rows,
            |_| {
                let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(count, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(1));
                active.fetch_sub(1, Ordering::SeqCst);
                Some("/nix/store/00000000000000000000000000000000-system".into())
            },
            &cancel,
        )
        .unwrap();
        assert_eq!(paths.as_object().unwrap().len(), 100);
        assert!(peak.load(Ordering::SeqCst) <= 4);
        cancel.store(1, Ordering::SeqCst);
        assert!(identities(
            &rows,
            |_| panic!("Must not resolve after cancellation"),
            &cancel
        )
        .is_err());
        assert!(identities(
            &rows,
            |_| Some("/tmp/not-store".into()),
            &AtomicUsize::new(0)
        )
        .unwrap()
        .as_object()
        .unwrap()
        .is_empty());
    }
    #[test]
    fn invalid_review_identities_fail_before_any_probe() {
        for args in [
            vec![],
            vec!["0", "target", "running"],
            vec!["1", "../target", "running"],
            vec![
                "+1",
                "00000000000000000000000000000000-target",
                "00000000000000000000000000000000-running",
            ],
        ] {
            assert!(reviewed(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
        }
    }
}
