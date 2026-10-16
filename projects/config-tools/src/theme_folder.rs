//! Drop-in theme folders: a `theme.json` and the files it names.
//!
//! A folder is a theme document of the contract (`theme_contract`) that also
//! carries the Base16 palette the desktop's other applications are themed
//! from, and names its own files relative to itself. `seele-theme register`
//! runs at build time: it applies every rule below, resolves those names to
//! the registered copy, and writes that copy, or refuses the folder and names
//! everything it broke, so a theme that is invalid or incomplete fails the
//! build instead of reaching a surface. The catalog then points at the
//! registered `theme.json`, and the helper holds it to the same rules again
//! whenever it reads the catalog.
use crate::theme_contract;
use crate::Result;
use seele_runtime::fs::read_bounded;
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

const DOCUMENT_LIMIT: usize = 65536;
const FILE_LIMIT: u64 = 16 * 1024 * 1024;
pub const BASE16_KEYS: [&str; 16] = [
    "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08",
    "base09", "base0A", "base0B", "base0C", "base0D", "base0E", "base0F",
];

fn opaque(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

/// Base16 colours as Stylix writes them back, so the palette the flake renders
/// the applications from, the catalog and the document are one string.
fn base16_color(value: &str) -> bool {
    opaque(value) && !value.bytes().any(|byte| byte.is_ascii_uppercase())
}

/// What a folder must hold beyond the contract's own rules, which are about
/// the shell alone: the Base16 palette, opaque roles, and no key the helper
/// writes itself. A theme's own wallpaper is refused until the desktop draws
/// one, because the publication would otherwise replace it with the
/// configured image without a word.
fn folder_violations(object: &Map<String, Value>) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    match object.get("palette") {
        None => errors.push(
            "missing palette, the 16 Base16 colours the desktop's other applications are themed from"
                .to_owned(),
        ),
        Some(Value::Object(palette)) => {
            for key in BASE16_KEYS {
                match palette.get(key) {
                    None => errors.push(format!("palette.{key}: missing")),
                    Some(value) if !value.as_str().is_some_and(base16_color) => {
                        errors.push(format!("palette.{key}: {value} is not a lowercase #rrggbb"))
                    }
                    Some(_) => {}
                }
            }
            for key in palette.keys() {
                if !BASE16_KEYS.contains(&key.as_str()) {
                    errors.push(format!("palette.{key}: not a Base16 colour"));
                }
            }
        }
        Some(value) => errors.push(format!("palette: {value} is not an object of Base16 colours")),
    }
    for role in theme_contract::roles()? {
        if let Some(Value::String(value)) = object.get(role) {
            // A colour the contract refuses is reported once, by the contract.
            if value.len() == 9 && !opaque(value) && value.starts_with('#') {
                errors.push(format!(
                    "{role}: \"{value}\" is translucent; a palette role is an opaque #rrggbb"
                ));
            }
        }
    }
    for key in ["terminal", "version"] {
        if object.contains_key(key) {
            errors.push(format!(
                "{key}: the helper adds it when the theme is published"
            ));
        }
    }
    if object.contains_key("wallpaper") {
        errors.push("wallpaper: a theme's own image is not drawn yet".to_owned());
    }
    Ok(errors)
}

fn report(kind: &str, id: &str, errors: &[String]) -> String {
    format!("Invalid theme {kind} {id}:\n  {}", errors.join("\n  "))
}

/// Where `name` leads inside `folder`, if it is a plain relative path that
/// stays there and names a file of a reasonable size.
fn resolve(
    folder: &Path,
    root: &Path,
    slot: &str,
    name: &str,
) -> std::result::Result<PathBuf, String> {
    let relative = Path::new(name);
    if relative.is_absolute()
        || relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "{slot}: \"{name}\" is not a path inside the theme folder"
        ));
    }
    let file = folder
        .join(relative)
        .canonicalize()
        .ok()
        .filter(|file| file.starts_with(root))
        .and_then(|file| Some((fs::metadata(&file).ok()?, file)))
        .filter(|(metadata, _)| metadata.is_file());
    match file {
        None => Err(format!(
            "{slot}: \"{name}\" is not a file in the theme folder"
        )),
        Some((metadata, _)) if metadata.len() > FILE_LIMIT => {
            Err(format!("{slot}: \"{name}\" is larger than 16 MiB"))
        }
        Some((_, file)) => Ok(file),
    }
}

/// Checks the folder `id` names and writes its registered copy to `out`: the
/// document, with every file it names resolved below `out`, and those files.
/// Nothing is written unless every rule holds.
pub fn register(id: &str, folder: &Path, out: &Path) -> Result {
    let refuse = |errors: &[String]| -> Result { Err(report("folder", id, errors).into()) };
    if !out.is_absolute() {
        return Err("The registered copy needs an absolute path".into());
    }
    let bytes = match read_bounded(&folder.join("theme.json"), DOCUMENT_LIMIT, false) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return refuse(&["no theme.json".to_owned()]);
        }
        Err(_) => return refuse(&["theme.json is not a file of at most 64 KiB".to_owned()]),
    };
    let mut document: Value = match serde_json::from_slice(&bytes) {
        Ok(document) => document,
        Err(error) => return refuse(&[format!("theme.json is not JSON: {error}")]),
    };
    let Some(object) = document.as_object_mut() else {
        return refuse(&["a theme document is a JSON object".to_owned()]);
    };
    let mut errors = Vec::new();
    if let Some(Value::String(named)) = object.get("id") {
        if named != id {
            errors.push(format!("id: \"{named}\" is not the folder's name"));
        }
    }
    errors.extend(folder_violations(object)?);
    let root = folder
        .canonicalize()
        .map_err(|_| report("folder", id, &["the folder cannot be read".to_owned()]))?;
    let mut files = BTreeMap::new();
    for slot in theme_contract::paths()? {
        let Some(Value::String(name)) = object.get(slot) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        match resolve(folder, &root, slot, name) {
            Ok(file) => {
                files.insert(name.clone(), file);
                object.insert(
                    slot.to_owned(),
                    out.join(name).to_string_lossy().into_owned().into(),
                );
            }
            Err(error) => {
                errors.push(error);
                // Already reported; the contract would only repeat it.
                object.remove(slot);
            }
        }
    }
    errors.extend(theme_contract::violations(&document)?);
    if !errors.is_empty() {
        return refuse(&errors);
    }
    fs::create_dir_all(out)?;
    for (name, file) in &files {
        let target = out.join(name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(file, &target)?;
    }
    let mut text = serde_json::to_vec_pretty(&document)?;
    text.push(b'\n');
    fs::write(out.join("theme.json"), text)?;
    Ok(())
}

/// A registered document, as the catalog entry that names it describes it.
/// It is held to the rules registration applied, and has to agree with its
/// entry, which the flake builds from the same folder.
pub fn load(
    path: &Path,
    id: &str,
    name: &str,
    mode: &str,
    palette: &BTreeMap<String, String>,
) -> Result<Map<String, Value>> {
    let refuse = |errors: &[String]| -> Result<Map<String, Value>> {
        Err(report("document", id, errors).into())
    };
    if !path.is_absolute() {
        return refuse(&["its registered document must be an absolute path".to_owned()]);
    }
    let document: Value = match read_bounded(path, DOCUMENT_LIMIT, false)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(document) => document,
        None => return refuse(&["its registered document cannot be read".to_owned()]),
    };
    let Value::Object(object) = document else {
        return refuse(&["a theme document is a JSON object".to_owned()]);
    };
    let mut errors = theme_contract::violations(&Value::Object(object.clone()))?;
    errors.extend(folder_violations(&object)?);
    for (key, expected) in [("id", id), ("name", name), ("mode", mode)] {
        if object.get(key).and_then(Value::as_str) != Some(expected) {
            errors.push(format!("{key}: not the catalog's {expected:?}"));
        }
    }
    let listed: Option<BTreeMap<String, String>> = object
        .get("palette")
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    if listed.as_ref() != Some(palette) {
        errors.push("palette: not the catalog's".to_owned());
    }
    for slot in theme_contract::paths()? {
        if let Some(Value::String(file)) = object.get(slot) {
            if !file.is_empty() && !Path::new(file).is_file() {
                errors.push(format!("{slot}: \"{file}\" is missing"));
            }
        }
    }
    if !errors.is_empty() {
        return refuse(&errors);
    }
    Ok(object)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn palette() -> Value {
        Value::Object(
            BASE16_KEYS
                .iter()
                .enumerate()
                .map(|(index, key)| {
                    (
                        (*key).to_owned(),
                        json!(format!("#{:06x}", index * 0x0a0a0a)),
                    )
                })
                .collect(),
        )
    }
    fn example() -> Value {
        json!({
            "id": "example", "name": "Example", "mode": "dark",
            "base": "#101820", "mantle": "#0c1218", "crust": "#080c10",
            "surface": "#1c2a36", "overlay": "#5c7080", "text": "#e8f0f8",
            "subtext": "#a8b8c8", "accent": "#f2aa4c", "red": "#ff6b6b",
            "green": "#7bd88f", "yellow": "#ffd866",
            "palette": palette(),
            "radius": 14,
        })
    }
    struct Folder {
        directory: tempfile::TempDir,
    }
    impl Folder {
        fn new(document: &Value) -> Self {
            let directory = tempfile::tempdir().unwrap();
            fs::create_dir(directory.path().join("example")).unwrap();
            let folder = Folder { directory };
            folder.write("theme.json", &serde_json::to_vec(document).unwrap());
            folder
        }
        fn path(&self) -> PathBuf {
            self.directory.path().join("example")
        }
        fn out(&self) -> PathBuf {
            self.directory.path().join("out")
        }
        fn write(&self, name: &str, bytes: &[u8]) {
            let file = self.path().join(name);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        fn register(&self) -> std::result::Result<Value, String> {
            register("example", &self.path(), &self.out()).map_err(|error| error.to_string())?;
            Ok(serde_json::from_slice(&fs::read(self.out().join("theme.json")).unwrap()).unwrap())
        }
    }
    fn with(key: &str, value: Value) -> Value {
        let mut document = example();
        document[key] = value;
        document
    }
    fn without(key: &str) -> Value {
        let mut document = example();
        document.as_object_mut().unwrap().remove(key);
        document
    }
    fn refuses(document: Value, message: &str) {
        let folder = Folder::new(&document);
        let error = folder.register().unwrap_err();
        assert!(
            error.starts_with("Invalid theme folder example:\n  "),
            "{error}"
        );
        assert!(
            error
                .lines()
                .skip(1)
                .any(|line| line.trim_start().starts_with(message)),
            "expected {message:?} in {error}"
        );
        assert!(!folder.out().exists(), "a refused folder writes nothing");
    }

    #[test]
    fn a_complete_folder_registers_as_it_is_written() {
        let folder = Folder::new(&example());
        assert_eq!(folder.register().unwrap(), example());
        let entries: Vec<_> = fs::read_dir(folder.out()).unwrap().collect();
        assert_eq!(entries.len(), 1, "only the document");
    }

    #[test]
    fn named_files_are_resolved_into_the_registered_copy() {
        let mut document = example();
        document["soundNotification"] = json!("sounds/ping.oga");
        document["soundFocusDone"] = json!("");
        let folder = Folder::new(&document);
        folder.write("sounds/ping.oga", b"OggS");
        folder.write("notes.txt", b"not named, not copied");
        let registered = folder.register().unwrap();
        let ping = folder.out().join("sounds/ping.oga");
        assert_eq!(
            registered["soundNotification"],
            json!(ping.to_str().unwrap())
        );
        assert_eq!(registered["soundFocusDone"], json!(""));
        assert_eq!(fs::read(&ping).unwrap(), b"OggS");
        assert!(!folder.out().join("notes.txt").exists());
    }

    #[test]
    fn every_broken_rule_is_named_at_once() {
        let mut document = without("accent");
        document["radius"] = json!("round");
        document["palette"]
            .as_object_mut()
            .unwrap()
            .remove("base0D");
        let folder = Folder::new(&document);
        let error = folder.register().unwrap_err();
        assert_eq!(
            error,
            "Invalid theme folder example:\n  palette.base0D: missing\n  missing required slot accent\n  radius: \"round\" is not a int"
        );
    }

    #[test]
    fn an_incomplete_or_invalid_folder_is_refused() {
        refuses(without("palette"), "missing palette");
        refuses(
            with("palette", json!(["#000000"])),
            "palette: [\"#000000\"] is not an object",
        );
        refuses(
            with("palette", {
                let mut palette = palette();
                palette["base05"] = json!("white");
                palette["base10"] = json!("#000000");
                palette
            }),
            "palette.base05: \"white\" is not a lowercase #rrggbb",
        );
        refuses(
            with("palette", {
                let mut palette = palette();
                palette["base10"] = json!("#000000");
                palette
            }),
            "palette.base10: not a Base16 colour",
        );
        refuses(
            with("palette", {
                let mut palette = palette();
                palette["base0D"] = json!("#F2AA4C");
                palette
            }),
            "palette.base0D: \"#F2AA4C\" is not a lowercase #rrggbb",
        );
        refuses(
            with("id", json!("other")),
            "id: \"other\" is not the folder's name",
        );
        refuses(with("id", json!("Not An Id")), "id: \"Not An Id\"");
        refuses(without("name"), "missing name");
        refuses(with("mode", json!("dim")), "mode");
        refuses(without("yellow"), "missing required slot yellow");
        refuses(
            with("accent", json!("#80f2aa4c")),
            "accent: \"#80f2aa4c\" is translucent",
        );
        refuses(
            with("accent", json!("orange")),
            "accent: \"orange\" is not a color",
        );
        refuses(with("panelColor", json!("#000000")), "panelColor: derived");
        refuses(with("sparkle", json!(true)), "sparkle: not a contract slot");
        refuses(
            with("terminal", json!(["#000000"])),
            "terminal: the helper adds it",
        );
        refuses(with("version", json!(2)), "version: the helper adds it");
        refuses(
            with("wallpaper", json!("wallpaper.jpg")),
            "wallpaper: a theme's own image",
        );
        refuses(
            json!(["not", "an", "object"]),
            "a theme document is a JSON object",
        );
    }

    #[test]
    fn a_named_file_must_be_inside_the_folder() {
        for (name, message) in [
            ("missing.oga", "is not a file in the theme folder"),
            ("/etc/hostname", "is not a path inside the theme folder"),
            ("../escape.oga", "is not a path inside the theme folder"),
            ("./ping.oga", "is not a path inside the theme folder"),
            ("sounds", "is not a file in the theme folder"),
            ("outside.oga", "is not a file in the theme folder"),
        ] {
            let folder = Folder::new(&with("soundNotification", json!(name)));
            folder.write("sounds/ping.oga", b"OggS");
            fs::write(folder.directory.path().join("escape.oga"), b"OggS").unwrap();
            std::os::unix::fs::symlink(
                folder.directory.path().join("escape.oga"),
                folder.path().join("outside.oga"),
            )
            .unwrap();
            let error = folder.register().unwrap_err();
            assert!(
                error.contains(&format!("soundNotification: \"{name}\" {message}")),
                "{name}: {error}"
            );
            assert_eq!(error.lines().count(), 2, "reported once: {error}");
        }
    }

    #[test]
    fn a_folder_without_a_readable_document_says_so() {
        let folder = Folder::new(&example());
        fs::remove_file(folder.path().join("theme.json")).unwrap();
        assert_eq!(
            folder.register().unwrap_err(),
            "Invalid theme folder example:\n  no theme.json"
        );
        folder.write("theme.json", b"{\"id\": ");
        assert!(folder
            .register()
            .unwrap_err()
            .starts_with("Invalid theme folder example:\n  theme.json is not JSON: "));
    }

    #[test]
    fn a_registered_document_loads_only_as_its_catalog_entry_describes_it() {
        let folder = Folder::new(&example());
        folder.register().unwrap();
        let path = folder.out().join("theme.json");
        let palette: BTreeMap<String, String> = serde_json::from_value(palette()).unwrap();
        let loaded = load(&path, "example", "Example", "dark", &palette).unwrap();
        assert_eq!(Value::Object(loaded), example());
        let refused = |id, name, mode, palette: &BTreeMap<String, String>| {
            load(&path, id, name, mode, palette)
                .unwrap_err()
                .to_string()
        };
        assert!(refused("other", "Example", "dark", &palette)
            .contains("id: not the catalog's \"other\""));
        assert!(
            refused("example", "Example", "light", &palette).contains("mode: not the catalog's")
        );
        let mut changed = palette.clone();
        changed.insert("base00".to_owned(), "#ffffff".to_owned());
        assert!(
            refused("example", "Example", "dark", &changed).contains("palette: not the catalog's")
        );
        assert!(load(
            Path::new("theme.json"),
            "example",
            "Example",
            "dark",
            &palette
        )
        .is_err());
        fs::write(&path, b"{}").unwrap();
        assert!(
            refused("example", "Example", "dark", &palette).contains("missing required slot base")
        );
    }
}
