use crate::Result;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Catalog {
    system: String,
    applications: Vec<Application>,
}
#[derive(serde::Serialize, Deserialize)]
struct Application {
    name: String,
    binary: String,
    modules: Vec<String>,
    systems: Vec<String>,
}

fn validate(catalog: &Catalog) -> Result {
    let short = |value: &str| value.chars().count() <= 200;
    if catalog.applications.len() > 4096
        || !short(&catalog.system)
        || catalog.applications.iter().any(|app| {
            !short(&app.name)
                || !short(&app.binary)
                || app.modules.len() > 64
                || app.systems.len() > 16
                || app
                    .modules
                    .iter()
                    .chain(&app.systems)
                    .any(|value| !short(value))
        })
    {
        return Err("catalog record exceeds its limit".into());
    }
    Ok(())
}
fn plain(catalog: &mut Catalog) {
    catalog.system = crate::text::terminal(&catalog.system);
    for app in &mut catalog.applications {
        app.name = crate::text::terminal(&app.name);
        app.binary = crate::text::terminal(&app.binary);
        for value in app.modules.iter_mut().chain(&mut app.systems) {
            *value = crate::text::terminal(value);
        }
    }
}

pub fn run(path: &Path, args: &[String]) -> Result {
    let (mut all, mut json, mut selected) = (false, false, None);
    for arg in args {
        match arg.as_str() {
            "--all-systems" => all = true,
            "--json" => json = true,
            "--help" | "-h" => {
                println!("Discover Seele's configured portable applications.\nUsage: seele-portable-apps [--all-systems] [--json] [COMMAND]");
                return Ok(());
            }
            value if !value.starts_with('-') && selected.is_none() => selected = Some(value),
            _ => return Err("invalid catalog argument".into()),
        }
    }
    let bytes = seele_runtime::fs::read_bounded(path, 1024 * 1024, false)?;
    let mut catalog: Catalog = serde_json::from_slice(&bytes)?;
    validate(&catalog)?;
    catalog.applications.sort_by(|a, b| a.name.cmp(&b.name));
    catalog.applications.retain(|app| {
        (all || app.systems.contains(&catalog.system))
            && selected.is_none_or(|selected| app.name == selected)
    });
    if selected.is_some() && catalog.applications.is_empty() {
        return Err("no portable command in this selection; try --all-systems".into());
    }
    if !json {
        plain(&mut catalog);
    }
    if json {
        println!("{}", serde_json::to_string_pretty(&catalog.applications)?);
    } else if let Some(app) = catalog.applications.first().filter(|_| selected.is_some()) {
        println!("{} (executable: {})\nFeatures: {}\nPlatforms: {}\nRun: nix run github:silas00301/seele#{}",app.name,app.binary,app.modules.join(", "),app.systems.join(", "),app.name);
    } else {
        println!(
            "Seele portable applications — {}\n",
            if all {
                "all platforms"
            } else {
                &catalog.system
            }
        );
        let name_width = catalog
            .applications
            .iter()
            .map(|app| app.name.chars().count())
            .max()
            .unwrap_or(0)
            .max(7);
        let bin_width = catalog
            .applications
            .iter()
            .map(|app| app.binary.chars().count())
            .max()
            .unwrap_or(0)
            .max(10);
        println!(
            "{:name_width$}  {:bin_width$}  INCLUDED FEATURES",
            "COMMAND", "EXECUTABLE"
        );
        for app in catalog.applications {
            println!(
                "{:name_width$}  {:bin_width$}  {}",
                app.name,
                app.binary,
                app.modules.join(", ")
            );
        }
        println!("\nRun: nix run github:silas00301/seele#COMMAND\nInspect a command: nix run github:silas00301/seele#portable-apps -- COMMAND");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn terminal_fields_are_escaped_and_padding_amplification_is_bounded() {
        let mut catalog = Catalog {
            system: "system\x1b]52;clipboard\x07".into(),
            applications: vec![Application {
                name: "app\nspoof".into(),
                binary: "bin\u{202e}name".into(),
                modules: vec!["module\tspoof".into()],
                systems: vec!["linux".into()],
            }],
        };
        validate(&catalog).unwrap();
        plain(&mut catalog);
        for value in [
            &catalog.system,
            &catalog.applications[0].name,
            &catalog.applications[0].binary,
            &catalog.applications[0].modules[0],
        ] {
            assert!(!value.contains(['\x1b', '\x07', '\n', '\t', '\u{202e}']));
        }
        catalog.applications[0].name = "x".repeat(201);
        assert!(validate(&catalog).is_err());
    }
}
