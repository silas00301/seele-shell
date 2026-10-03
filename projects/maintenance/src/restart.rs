//! What a NixOS switch leaves running from boot. `switch-to-configuration`
//! activates a generation in place, but some of it is only read while the
//! machine starts, and some services are deliberately never restarted because
//! restarting them would end the session. This probe compares the booted
//! generation with the current one, part by part, and names what a restart
//! into the current generation would still change. It reads world-readable
//! symlinks and small generated files only; no part of it needs privilege.
use crate::model::{Finding, Result, Urgency};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

pub const BOOTED: &str = "/run/booted-system";
pub const CURRENT: &str = "/run/current-system";
/// One condition per machine: the booted generation differs from the current
/// one in a part only a boot applies.
pub const KEY: &str = "booted-system";
const FILE_LIMIT: usize = 64 * 1024;
const WALK_LIMIT: usize = 16384;
const LIST_LIMIT: usize = 4;

fn missing(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}
/// The target of one of a generation's top-level links. Absence is a value
/// (a generation without an initrd has none); any other error fails the probe.
fn link(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    match fs::read_link(root.join(name)) {
        Ok(target) => Ok(Some(target)),
        Err(error) if missing(&error) => Ok(None),
        Err(_) => Err("probe_unavailable"),
    }
}
fn file(root: &Path, name: &str) -> Result<Option<String>> {
    match crate::read_bounded(&root.join(name), FILE_LIMIT, false) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|_| "invalid_probe_snapshot"),
        Err(error) if missing(&error) => Ok(None),
        Err(_) => Err("probe_unavailable"),
    }
}

/// The name a store path was built under, without its hash: the component
/// after the store directory, `<32 hash characters>-<name>`. The store
/// directory itself is not assumed, so fixtures can live anywhere.
pub fn store_name(path: &Path) -> Option<String> {
    path.components().find_map(|component| {
        let text = component.as_os_str().to_str()?;
        let (hash, name) = text.split_at_checked(32)?;
        let name = name.strip_prefix('-')?;
        (hash
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
            && !name.is_empty()
            && name.len() <= 211
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"+-._?=".contains(&b)))
        .then(|| name.to_owned())
    })
}
/// Nix's own split: the version starts at the first dash followed by a digit.
pub fn split_name(name: &str) -> (&str, &str) {
    name.char_indices()
        .find(|(index, c)| {
            *c == '-'
                && name[index + 1..]
                    .chars()
                    .next()
                    .is_some_and(|n| n.is_ascii_digit())
        })
        .map(|(index, _)| (&name[..index], &name[index + 1..]))
        .unwrap_or((name, ""))
}
/// "systemd 257.5 → 257.6", "systemd 257.6, rebuilt", or both full names when
/// the package itself was replaced.
fn transition(old: &str, new: &str) -> String {
    let ((old_name, old_version), (new_name, new_version)) = (split_name(old), split_name(new));
    let named = |name: &str, version: &str| {
        if version.is_empty() {
            name.to_owned()
        } else {
            format!("{name} {version}")
        }
    };
    if old_name != new_name {
        format!(
            "{} → {}",
            named(old_name, old_version),
            named(new_name, new_version)
        )
    } else if old_version == new_version {
        format!("{}, rebuilt", named(new_name, new_version))
    } else if old_version.is_empty() || new_version.is_empty() {
        named(new_name, new_version)
    } else {
        format!("{new_name} {old_version} → {new_version}")
    }
}

/// The running kernel's `uname -r`, read from the one directory its module tree
/// carries. That names flavoured kernels exactly ("6.12.10-zen1").
fn release(root: &Path) -> Option<String> {
    let entries = fs::read_dir(root.join("kernel-modules/lib/modules")).ok()?;
    let names: Vec<String> = entries
        .take(8)
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    match names.as_slice() {
        [name]
            if name.len() <= 64
                && name.starts_with(|c: char| c.is_ascii_digit())
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+-._~".contains(&b)) =>
        {
            Some(name.clone())
        }
        _ => None,
    }
}
fn kernel_version(root: &Path, kernel: Option<&PathBuf>) -> Option<String> {
    release(root).or_else(|| {
        let name = store_name(kernel?)?;
        let version = split_name(&name).1;
        (!version.is_empty()).then(|| version.to_owned())
    })
}

/// Store packages a buildEnv-style tree links into, without following them.
/// `None` means the tree could not be read within its bounds; the caller then
/// falls back to naming the part without its packages.
fn packages(root: &Path, depth: usize) -> Option<BTreeSet<String>> {
    let mut found = BTreeSet::new();
    let mut pending = vec![(root.to_path_buf(), depth)];
    let mut visited = 0;
    while let Some((directory, remaining)) = pending.pop() {
        for entry in fs::read_dir(&directory).ok()? {
            visited += 1;
            if visited > WALK_LIMIT {
                return None;
            }
            let entry = entry.ok()?;
            let kind = entry.file_type().ok()?;
            if kind.is_symlink() {
                if let Some(name) = store_name(&fs::read_link(entry.path()).ok()?) {
                    found.insert(name);
                }
            } else if kind.is_dir() && remaining > 0 {
                pending.push((entry.path(), remaining - 1));
            }
        }
    }
    Some(found)
}
/// A package's own version, without what the store name appends to it:
/// firmware compression ("linux-firmware-20250911-zstd"), a firmware output
/// ("nvidia-x11-575.64-firmware", the GSP firmware), or the kernel version an
/// out-of-tree module is built against ("nvidia-open-575.64-6.12.10").
fn own_version(version: &str, kernel: &[String]) -> String {
    let mut version = version;
    loop {
        let shorter = ["-zstd", "-xz", "-firmware"]
            .iter()
            .copied()
            .chain(kernel.iter().map(String::as_str))
            .find_map(|suffix| version.strip_suffix(suffix));
        match shorter {
            Some(shorter) if !shorter.is_empty() => version = shorter,
            _ => return version.to_owned(),
        }
    }
}
/// Pairs what was removed and added by package name, so a driver update reads
/// as one line rather than as a removal and an addition.
fn package_changes(
    old: &BTreeSet<String>,
    new: &BTreeSet<String>,
    kernel: [&[String]; 2],
) -> Vec<String> {
    let mut by_name: BTreeMap<String, (Vec<String>, Vec<String>)> = BTreeMap::new();
    for (set, other, is_new) in [(old, new, false), (new, old, true)] {
        for name in set.difference(other) {
            let (package, version) = split_name(name);
            let entry = by_name.entry(package.to_owned()).or_default();
            let version = own_version(version, kernel[usize::from(is_new)]);
            if is_new {
                entry.1.push(version);
            } else {
                entry.0.push(version);
            }
        }
    }
    by_name
        .into_iter()
        .filter_map(
            |(package, (removed, added))| match (removed.first(), added.first()) {
                (Some(old), Some(new)) if old == new => None,
                (Some(old), Some(new)) => Some(format!("{package} {old} → {new}")),
                (None, Some(new)) => Some(format!("{package} {new} added")),
                (Some(_), None) => Some(format!("{package} removed")),
                (None, None) => None,
            },
        )
        .collect()
}
fn bounded(mut lines: Vec<String>) -> String {
    let more = lines.len().saturating_sub(LIST_LIMIT);
    lines.truncate(LIST_LIMIT);
    let mut text = lines.join(", ");
    if more > 0 {
        text.push_str(&format!(", and {more} more"));
    }
    text
}

/// Names parameters, never their values: a command line can carry device
/// identifiers that the finding has no reason to repeat.
fn parameters(old: &str, new: &str) -> Vec<String> {
    fn keyed(text: &str) -> BTreeMap<String, BTreeSet<&str>> {
        let mut keys: BTreeMap<String, BTreeSet<&str>> = BTreeMap::new();
        for token in text.split_whitespace() {
            let key: String = token
                .split('=')
                .next()
                .unwrap_or(token)
                .chars()
                .take(40)
                .collect();
            keys.entry(key).or_default().insert(token);
        }
        keys
    }
    let (old, new) = (keyed(old), keyed(new));
    old.keys()
        .chain(new.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|key| match (old.get(key), new.get(key)) {
            (None, Some(_)) => Some(format!("{key} added")),
            (Some(_), None) => Some(format!("{key} removed")),
            (Some(a), Some(b)) if a != b => Some(format!("{key} changed")),
            _ => None,
        })
        .collect()
}
fn inhibitors(root: &Path) -> Result<BTreeMap<String, String>> {
    let Some(text) = file(root, "switch-inhibitors")? else {
        return Ok(BTreeMap::new());
    };
    let value: Value = serde_json::from_str(&text).map_err(|_| "invalid_probe_snapshot")?;
    value
        .as_object()
        .ok_or("invalid_probe_snapshot")?
        .iter()
        .map(|(key, value)| {
            value
                .as_str()
                .map(|value| (key.clone(), value.to_owned()))
                .ok_or("invalid_probe_snapshot")
        })
        .collect()
}
/// The bus binary each generation actually runs, chosen by the implementation
/// that generation declares; both packages are installed under `broker`.
fn bus(root: &Path, declared: &BTreeMap<String, String>) -> Result<Option<String>> {
    let binary = if declared.get("dbus-implementation").map(String::as_str) == Some("broker") {
        "sw/bin/dbus-broker"
    } else {
        "sw/bin/dbus-daemon"
    };
    Ok(link(root, binary)?.and_then(|target| store_name(&target)))
}

/// Every boot-only difference, in human terms, most consequential first.
pub fn changes(booted: &Path, current: &Path) -> Result<Vec<String>> {
    let (old_root, new_root) = (
        fs::canonicalize(booted).map_err(|_| "probe_unavailable")?,
        fs::canonicalize(current).map_err(|_| "probe_unavailable")?,
    );
    if old_root == new_root {
        return Ok(vec![]);
    }
    let (booted, current) = (old_root.as_path(), new_root.as_path());
    let mut changes = vec![];
    let differs = |name: &str| -> Result<Option<(Option<PathBuf>, Option<PathBuf>)>> {
        let (old, new) = (link(booted, name)?, link(current, name)?);
        Ok((old != new).then_some((old, new)))
    };

    // The kernel image and initrd are what the boot loader hands over.
    let (old_kernel, new_kernel) = (link(booted, "kernel")?, link(current, "kernel")?);
    let kernel = old_kernel != new_kernel;
    let (old_release, new_release) = (
        kernel_version(booted, old_kernel.as_ref()),
        kernel_version(current, new_kernel.as_ref()),
    );
    if kernel {
        changes.push(match (&old_release, &new_release) {
            (Some(old), Some(new)) if old == new => format!("Linux {new}, rebuilt"),
            (Some(old), Some(new)) => format!("Linux {old} → {new}"),
            _ => "the Linux kernel".into(),
        });
    }
    // Versions an out-of-tree module or its firmware carries for the kernel it
    // was built against, so a driver reads as its own version.
    let own = |kernel: &Option<PathBuf>| kernel.as_deref().and_then(store_name);
    let (old_own, new_own) = (own(&old_kernel), own(&new_kernel));
    let suffixes = |release: &Option<String>, own: &Option<String>| {
        let version = own.as_deref().map(|own| split_name(own).1);
        release
            .as_deref()
            .into_iter()
            .chain(version.filter(|v| !v.is_empty()))
            .map(|v| format!("-{v}"))
            .collect::<Vec<_>>()
    };
    let suffixes = [
        suffixes(&old_release, &old_own),
        suffixes(&new_release, &new_own),
    ];
    let suffixes = [suffixes[0].as_slice(), suffixes[1].as_slice()];
    // The running kernel only loads modules built for itself, so updated
    // out-of-tree drivers wait for the matching boot. NVIDIA's are
    // `nvidia-open` or `nvidia-kernel-modules`, installed under `updates/`.
    if differs("kernel-modules")?.is_some() {
        let tree = |root: &Path, own: &Option<String>| {
            packages(&root.join("kernel-modules/lib/modules"), 4).map(|set| {
                set.into_iter()
                    .filter(|name| {
                        own.as_ref()
                            .is_none_or(|own| !name.starts_with(own.as_str()))
                    })
                    .collect::<BTreeSet<_>>()
            })
        };
        match (tree(booted, &old_own), tree(current, &new_own)) {
            (Some(old), Some(new)) => {
                let lines = package_changes(&old, &new, suffixes);
                if !lines.is_empty() {
                    changes.push(format!("kernel modules ({})", bounded(lines)));
                } else if !kernel {
                    changes.push("kernel modules, rebuilt".into());
                }
            }
            _ if !kernel => changes.push("kernel modules".into()),
            _ => {}
        }
    }
    // A new kernel always brings a new initrd; only name it on its own.
    if !kernel && differs("initrd")?.is_some() {
        changes.push("early boot image (initrd)".into());
    }
    let (old_parameters, new_parameters) = (
        file(booted, "kernel-params")?.unwrap_or_default(),
        file(current, "kernel-params")?.unwrap_or_default(),
    );
    let lines = parameters(&old_parameters, &new_parameters);
    if !lines.is_empty() {
        changes.push(format!("kernel command line ({})", bounded(lines)));
    }
    // The activation script points the firmware loader at the new files, but
    // a driver that already loaded its firmware keeps it until it probes again.
    if differs("firmware")?.is_some() {
        let named = match (
            packages(&booted.join("firmware"), 2),
            packages(&current.join("firmware"), 2),
        ) {
            (Some(old), Some(new)) => package_changes(&old, &new, suffixes),
            _ => vec![],
        };
        changes.push(if named.is_empty() {
            "device firmware".into()
        } else {
            format!("device firmware ({})", bounded(named))
        });
    }
    // switch-to-configuration re-executes PID 1 and the user managers, but
    // NixOS never restarts systemd-logind, so the login manager keeps running
    // the booted systemd.
    if let Some((Some(old), Some(new))) = differs("systemd")? {
        if let (Some(old), Some(new)) = (store_name(&old), store_name(&new)) {
            changes.push(format!("login manager ({})", transition(&old, &new)));
        }
    }
    // NixOS only reloads the system bus; restarting it would end the session.
    let (old_declared, new_declared) = (inhibitors(booted)?, inhibitors(current)?);
    if let (Some(old), Some(new)) = (bus(booted, &old_declared)?, bus(current, &new_declared)?) {
        if old != new {
            changes.push(format!("message bus ({})", transition(&old, &new)));
        }
    }
    // Modules declare these as parts a switch must not change in place.
    // Upstream compares keys both generations declare; so does this.
    for (key, old) in &old_declared {
        if let Some(new) = new_declared.get(key).filter(|new| *new != old) {
            let short = |text: &str| text.chars().take(60).collect::<String>();
            changes.push(format!("{} ({} → {})", short(key), short(old), short(new)));
        }
    }
    Ok(changes)
}
fn capitalized(text: &str) -> String {
    let mut characters = text.chars();
    characters
        .next()
        .map(|first| first.to_uppercase().chain(characters).collect())
        .unwrap_or_default()
}
/// One finding while a restart is outstanding, none once it is not. The title
/// leads with the most consequential change; the details list them all.
pub fn report(booted: &Path, current: &Path) -> Result<Option<Finding>> {
    let changes = changes(booted, current)?;
    let Some(first) = changes.first() else {
        return Ok(None);
    };
    let more = changes.len() - 1;
    let title = match more {
        0 => format!("Restart to apply {first}"),
        1 => format!("Restart to apply {first} and 1 more change"),
        n => format!("Restart to apply {first} and {n} more changes"),
    };
    crate::publishers::report(
        KEY,
        title,
        "The running system still uses these parts of the generation it booted. Switching cannot replace them; restarting into the current generation does.",
        changes
            .iter()
            .map(|change| capitalized(change))
            .collect::<Vec<_>>()
            .join("\n"),
        Urgency::Eventually,
        &["open-power", "recheck"],
    )
    .map(Some)
}
