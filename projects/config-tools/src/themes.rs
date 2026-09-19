//! One catalog, one durable selection, and generated app-only includes.
use crate::Result;
use seele_runtime::{
    fs::{atomic_write, private_directory, read_bounded},
    process::{capture, Limits},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    env, fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub palette: BTreeMap<String, String>,
    pub vicinae_theme: PathBuf,
}

const BASE16_KEYS: [&str; 16] = [
    "base00", "base01", "base02", "base03", "base04", "base05", "base06", "base07", "base08",
    "base09", "base0A", "base0B", "base0C", "base0D", "base0E", "base0F",
];

// One projection bridges Base16's semantic slots to existing Seele/app roles.
#[derive(Serialize)]
struct Colors<'a> {
    base: &'a str,
    mantle: &'a str,
    crust: &'a str,
    surface: &'a str,
    overlay: &'a str,
    text: &'a str,
    subtext: &'a str,
    accent: &'a str,
    red: &'a str,
    green: &'a str,
    yellow: &'a str,
    terminal: [&'a str; 16],
}
impl Theme {
    fn colors(&self) -> Colors<'_> {
        let p = &self.palette;
        Colors {
            base: &p["base00"],
            mantle: &p["base01"],
            crust: &p["base00"],
            surface: &p["base02"],
            overlay: &p["base03"],
            text: &p["base05"],
            subtext: &p["base04"],
            accent: &p["base0D"],
            red: &p["base08"],
            green: &p["base0B"],
            yellow: &p["base0A"],
            terminal: [
                "base00", "base08", "base0B", "base0A", "base0D", "base0E", "base0C", "base05",
                "base03", "base08", "base0B", "base0A", "base0D", "base0E", "base0C", "base07",
            ]
            .map(|key| p[key].as_str()),
        }
    }
    fn display(&self) -> Result<serde_json::Value> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .ok_or("Invalid theme")?
            .remove("vicinaeTheme");
        if let serde_json::Value::Object(colors) = serde_json::to_value(self.colors())? {
            value.as_object_mut().ok_or("Invalid theme")?.extend(colors);
        }
        Ok(value)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Catalog {
    version: u32,
    default: String,
    font_family: String,
    wallpaper: String,
    themes: Vec<Theme>,
    commands: BTreeMap<String, PathBuf>,
}
fn valid_color(s: &str) -> bool {
    s.len() == 7 && s.starts_with('#') && s.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}
impl Catalog {
    fn validate(&self) -> Result {
        if self.version != 2 || self.themes.is_empty() || self.themes.len() > 32 {
            return Err("Invalid theme catalog".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        for t in &self.themes {
            if !matches!(t.mode.as_str(), "light" | "dark")
                || t.id.is_empty()
                || t.id.len() > 64
                || !t
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
                || !ids.insert(&t.id)
                || t.name.is_empty()
                || t.name.len() > 80
                || t.name.chars().any(char::is_control)
                || t.palette.len() != BASE16_KEYS.len()
                || BASE16_KEYS
                    .iter()
                    .any(|key| !t.palette.get(*key).is_some_and(|c| valid_color(c)))
                || !t.vicinae_theme.is_absolute()
            {
                return Err("Invalid theme palette".into());
            }
        }
        if !ids.contains(&self.default) {
            return Err("Default theme is missing".into());
        }
        if self.commands.values().any(|p| !p.is_absolute()) {
            return Err("Theme tools must have absolute paths".into());
        }
        Ok(())
    }
    fn theme(&self, id: &str) -> Result<&Theme> {
        self.themes
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| "Unknown theme".into())
    }
}
fn xdg(key: &str, fallback: &str) -> Result<PathBuf> {
    if let Some(value) = env::var_os(key).filter(|v| !v.is_empty()) {
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err("XDG paths must be absolute".into());
        }
        return Ok(path);
    }
    let home = PathBuf::from(env::var_os("HOME").ok_or("HOME is missing")?);
    if !home.is_absolute() {
        return Err("HOME must be absolute".into());
    }
    Ok(home.join(fallback))
}
fn selected(state: &Path, catalog: &Catalog) -> Result<String> {
    match read_bounded(&state.join("selection.json"), 32768, true) {
        Ok(bytes) => {
            let value: serde_json::Value = serde_json::from_slice(&bytes)?;
            let id = value
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or("Invalid saved theme")?;
            catalog.theme(id)?;
            Ok(id.into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(catalog.default.clone()),
        Err(e) => Err(e.into()),
    }
}
fn files(t: &Theme) -> BTreeMap<&'static str, String> {
    let c = t.colors();
    let mut files = BTreeMap::new();
    let mut ghostty = format!("background = {}\nforeground = {}\ncursor-color = {}\nselection-background = {}\nselection-foreground = {}\n", c.base, c.text, c.accent, c.surface, c.text);
    for (index, color) in c.terminal.iter().enumerate() {
        ghostty.push_str(&format!("palette = {index}={color}\n"));
    }
    files.insert("ghostty", ghostty);
    files.insert("fish.fish", format!("set -g fish_color_normal {}\nset -g fish_color_command {}\nset -g fish_color_param {}\nset -g fish_color_quote {}\nset -g fish_color_error {}\nset -g fish_color_comment {}\nset -g fish_color_operator {}\nset -g fish_color_escape {}\nset -g fish_color_autosuggestion {}\nset -g fish_color_search_match --background={}\nset -g fish_pager_color_prefix {}\nset -g fish_pager_color_completion {}\nset -g fish_pager_color_description {}\nset -g fish_pager_color_selected_background --background={}\n",
        &c.text[1..], &c.accent[1..], &c.text[1..], &c.green[1..], &c.red[1..], &c.overlay[1..], &c.accent[1..], &c.yellow[1..], &c.overlay[1..], &c.surface[1..], &c.accent[1..], &c.text[1..], &c.subtext[1..], &c.surface[1..]));
    // Keep the existing tmux layout and plugin-provided text; recolor their
    // semantic palette variables as well as the outer status/window surfaces.
    let mut tmux = format!("set -g status-style 'fg={},bg={}'\nset -g pane-border-style 'fg={}'\nset -g pane-active-border-style 'fg={}'\nset -g message-style 'fg={},bg={}'\nset -g mode-style 'fg={},bg={}'\n", c.text, c.base, c.surface, c.accent, c.text, c.surface, c.base, c.accent);
    for (key, color) in [
        ("bg", &c.base),
        ("fg", &c.text),
        ("surface_0", &c.surface),
        ("surface_1", &c.surface),
        ("surface_2", &c.overlay),
        ("overlay_0", &c.overlay),
        ("overlay_1", &c.overlay),
        ("overlay_2", &c.subtext),
        ("red", &c.red),
        ("green", &c.green),
        ("yellow", &c.yellow),
        ("blue", &c.terminal[4]),
        ("mauve", &c.terminal[5]),
        ("lavender", &c.accent),
    ] {
        tmux.push_str(&format!("set -g @thm_{key} '{color}'\n"));
    }
    files.insert("tmux.conf", tmux);
    files.insert("gtk.css", format!("@define-color theme_bg_color {};\n@define-color theme_fg_color {};\n@define-color theme_base_color {};\n@define-color theme_text_color {};\n@define-color theme_selected_bg_color {};\n@define-color theme_selected_fg_color {};\n@define-color accent_color {};\n@define-color accent_bg_color {};\n@define-color accent_fg_color {};\n@define-color window_bg_color {};\n@define-color window_fg_color {};\n@define-color view_bg_color {};\n@define-color view_fg_color {};\n@define-color headerbar_bg_color {};\n@define-color headerbar_fg_color {};\n@define-color card_bg_color {};\n@define-color popover_bg_color {};\n@define-color popover_fg_color {};\n", c.base, c.text, c.mantle, c.text, c.accent, c.base, c.accent, c.accent, c.base, c.base, c.text, c.mantle, c.text, c.mantle, c.text, c.surface, c.mantle, c.text));
    files.insert("hyprland.lua", format!("hl.config({{general = {{col = {{active_border = 'rgba({}ff)', inactive_border = 'rgba({}ff)'}}}}}})\n", &c.accent[1..], &c.surface[1..]));
    files
}
fn tool(catalog: &Catalog, name: &str, args: &[&str]) -> bool {
    let Some(path) = catalog.commands.get(name) else {
        return false;
    };
    let result = capture(
        Command::new(path).args(args),
        &[],
        Limits {
            timeout: Duration::from_secs(2),
            output: 16384,
        },
        &std::sync::atomic::AtomicBool::new(false),
    );
    result.is_ok_and(|output| {
        output.status.success()
            && (name != "hyprctl"
                || !String::from_utf8_lossy(&output.stdout)
                    .to_lowercase()
                    .contains("error"))
    })
}
fn reload(catalog: &Catalog, state: &Path, t: &Theme) -> Vec<&'static str> {
    let mut pending = vec![];
    if env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some()
        && !tool(catalog, "hyprctl", &["eval", &files(t)["hyprland.lua"]])
    {
        pending.push("Window borders");
    }
    if tool(catalog, "tmux", &["has-session"])
        && !tool(
            catalog,
            "tmux",
            &[
                "source-file",
                state.join("current/tmux.conf").to_str().unwrap_or(""),
            ],
        )
    {
        pending.push("tmux");
    }
    if env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
        if tool(
            catalog,
            "systemctl",
            &[
                "--user",
                "is-active",
                "--quiet",
                "app-com.mitchellh.ghostty.service",
            ],
        ) && !tool(
            catalog,
            "systemctl",
            &["--user", "reload", "app-com.mitchellh.ghostty.service"],
        ) {
            pending.push("Ghostty");
        }
        if !tool(
            catalog,
            "gsettings",
            &[
                "set",
                "org.gnome.desktop.interface",
                "color-scheme",
                if t.mode == "light" {
                    "prefer-light"
                } else {
                    "prefer-dark"
                },
            ],
        ) {
            pending.push("Desktop color preference");
        }
        if !tool(catalog, "vicinae", &["vicinae://theme/set/seele-current"]) {
            pending.push("Vicinae");
        }
    }
    pending
}
fn apply(catalog: &Catalog, state: &Path, id: &str, live: bool) -> Result<serde_json::Value> {
    catalog.theme(id)?;
    let directory = private_directory(state)?;
    // Serialize competing pickers and activation; directory locks need no
    // replaceable lock file and release automatically with the descriptor.
    directory.lock()?;
    let saved = if live {
        id.to_string()
    } else {
        selected(state, catalog)?
    };
    let id = saved.as_str();
    let theme = catalog.theme(id)?;
    let generation = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .prefix(".theme-")
        .tempdir_in(state)?;
    for (name, content) in files(theme) {
        atomic_write(&generation.path().join(name), content.as_bytes())?;
    }
    // Only data generated by Stylix enters the launcher theme; never execute it.
    let launcher = read_bounded(&theme.vicinae_theme, 65536, false)?;
    if launcher.is_empty() || std::str::from_utf8(&launcher).is_err() {
        return Err("Invalid generated launcher theme".into());
    }
    atomic_write(&generation.path().join("vicinae.toml"), &launcher)?;
    let mut selection = theme.display()?;
    selection["version"] = 2.into();
    selection["fontFamily"] = catalog.font_family.clone().into();
    selection["wallpaper"] = catalog.wallpaper.clone().into();
    let current = state.join("current");
    let previous = match fs::read_link(&current) {
        Ok(p) if p.components().count() == 1 && p.to_string_lossy().starts_with(".theme-") => {
            Some(p)
        }
        Ok(_) => return Err("Unrecognized theme link".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    // Rename a fresh symlink into place; app includes see a whole generation.
    let pending = generation.path().join("next");
    symlink(
        generation
            .path()
            .file_name()
            .ok_or("Invalid theme directory")?,
        &pending,
    )?;
    fs::rename(&pending, &current)?;
    if let Err(error) = atomic_write(
        &state.join("selection.json"),
        &serde_json::to_vec(&selection)?,
    ) {
        if let Some(old) = &previous {
            symlink(old, &pending)?;
            fs::rename(&pending, &current)?;
        } else {
            fs::remove_file(&current)?;
        }
        return Err(error.into());
    }
    let _ = generation.keep();
    let pending = if live {
        reload(catalog, state, theme)
    } else {
        vec![]
    };
    if let Some(old) = previous {
        let _ = fs::remove_dir_all(state.join(old));
    }
    Ok(serde_json::json!({"id": id, "pending": pending}))
}
pub fn main() -> Result {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help") {
        println!("seele-theme list | current | set <id> | init | reset");
        return Ok(());
    }
    let catalog: Catalog = serde_json::from_slice(&read_bounded(
        &xdg("XDG_CONFIG_HOME", ".config")?.join("seele-theme/catalog.json"),
        131072,
        false,
    )?)?;
    catalog.validate()?;
    let state = xdg("XDG_STATE_HOME", ".local/state")?.join("seele-theme");
    let reply = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["list"] => {
            serde_json::json!({"current": selected(&state, &catalog)?, "themes": catalog.themes.iter().map(Theme::display).collect::<Result<Vec<_>>>()?})
        }
        ["current"] => serde_json::json!({"id": selected(&state, &catalog)?}),
        ["set", id] => apply(&catalog, &state, id, true)?,
        ["reset"] => apply(&catalog, &state, &catalog.default, true)?,
        ["init"] => apply(&catalog, &state, &selected(&state, &catalog)?, false)?,
        _ => return Err("Use: seele-theme list | current | set <id> | init | reset".into()),
    };
    println!("{}", serde_json::to_string(&reply)?);
    Ok(())
}
