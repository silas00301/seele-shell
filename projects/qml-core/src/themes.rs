//! Theme picker presentation: one normalized catalog, the grouped rows a panel
//! draws, and the wording for what a switch did and did not reach.
//!
//! Publication, reloading and every file the switch writes belong to
//! `seele-theme`. What is left here is what one open panel is looking at, and
//! it is kept in Rust so a palette never reaches a Qt colour property without
//! having been checked, and so the launcher and the panel group and describe
//! the same catalog the same way.
use crate::value::{array, number, text, trim};
use serde_json::{Value, json};

/// Every colour role the panel draws with. The native helper projects Base16
/// into exactly these, so a theme missing one of them is not a theme the panel
/// can render honestly.
const ROLES: [&str; 11] = [
    "base", "mantle", "crust", "surface", "overlay", "text", "subtext", "accent", "red", "green",
    "yellow",
];
const MAX_THEMES: usize = 32;
const MAX_WORDS: usize = 8;
const MAX_PENDING: usize = 8;

/// `#rrggbb` and nothing else. Qt would accept a colour name or an `#aarrggbb`
/// with a different channel order, so anything but the helper's own form is
/// refused rather than guessed at.
fn color(value: Option<&Value>) -> Option<String> {
    let raw = text(value);
    let value = trim(&raw);
    let valid = value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit);
    valid.then(|| value.to_owned())
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn mode_label(mode: &str) -> &'static str {
    if mode == "light" { "Light" } else { "Dark" }
}

/// One catalog entry, reduced to what a row needs and refused when incomplete.
fn theme(value: &Value) -> Option<Value> {
    let id = text(value.get("id"));
    let name = text(value.get("name"));
    let mode = text(value.get("mode"));
    if !identifier(&id)
        || name.is_empty()
        || name.chars().count() > 80
        || name.chars().any(char::is_control)
        || !matches!(mode.as_str(), "light" | "dark")
    {
        return None;
    }
    let mut row = json!({"id": id, "name": name, "mode": mode, "modeLabel": mode_label(&mode)});
    let object = row.as_object_mut()?;
    for role in ROLES {
        object.insert(role.to_owned(), json!(color(value.get(role))?));
    }
    Some(row)
}

fn matches(row: &Value, words: &[String]) -> bool {
    let haystack = format!(
        "{} {} {}",
        text(row.get("name")).to_lowercase(),
        text(row.get("id")),
        text(row.get("mode"))
    );
    words.iter().all(|word| haystack.contains(word))
}

/// A row's own four self-evident colours, so the delegate draws a preview
/// rather than choosing what a preview is.
fn swatches(row: &Value) -> Value {
    json!([
        text(row.get("accent")),
        text(row.get("red")),
        text(row.get("green")),
        text(row.get("yellow")),
    ])
}

fn failure(code: &str) -> &'static str {
    match code {
        "" => "",
        "unavailable" => "The theme helper is unavailable. Nothing was changed.",
        "invalid" => "The theme catalog could not be read. Nothing was changed.",
        "unknown" => "That theme is no longer in the catalog. Refresh the list.",
        "timeout" => "The theme helper did not answer. Refresh to see what is selected.",
        _ => "The theme could not be applied. Try again.",
    }
}

/// What is still wearing the old palette, named as a sentence. The switch is
/// saved by the time this is read, so it never says that anything failed.
fn pending(names: &[Value]) -> String {
    let names: Vec<String> = names
        .iter()
        .take(MAX_PENDING)
        .map(|value| {
            text(Some(value))
                .chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
        })
        .filter(|name| !name.is_empty())
        .collect();
    let list = match names.split_last() {
        None => return String::new(),
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
    };
    let verb = if names.len() == 1 { "shows" } else { "show" };
    format!("{list} still {verb} the previous palette.")
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    let first = args.first().unwrap_or(&null);
    Ok(match function {
        // The helper's `list` reply, accepted whole or not at all: a catalog
        // with one unusable palette in it would leave a row that cannot say
        // what applying it would look like.
        "catalog" => {
            let themes = array(first.get("themes"));
            if themes.is_empty() || themes.len() > MAX_THEMES {
                return Ok(json!({"ok": false, "error": "invalid"}));
            }
            let mut rows = Vec::with_capacity(themes.len());
            let mut ids = std::collections::BTreeSet::new();
            for value in themes {
                match theme(value) {
                    Some(row) if ids.insert(text(row.get("id"))) => rows.push(row),
                    _ => return Ok(json!({"ok": false, "error": "invalid"})),
                }
            }
            let current = text(first.get("current"));
            json!({
                "ok": true,
                // A selection the catalog does not contain marks nothing,
                // rather than marking whichever row happens to be first.
                "current": if ids.contains(&current) { current } else { String::new() },
                "themes": rows,
            })
        }
        // The rows a panel shows: the selection first, then the catalog's own
        // curated order inside each mode. Nothing is re-sorted alphabetically,
        // because the order presets were chosen in is information too.
        "rows" => {
            let themes = array(Some(first));
            let current = text(args.get(1));
            let query = text(args.get(2)).to_lowercase();
            let words: Vec<String> = trim(&query)
                .split_whitespace()
                .take(MAX_WORDS)
                .map(str::to_owned)
                .collect();
            let mut groups: [Vec<Value>; 3] = [vec![], vec![], vec![]];
            for row in themes.iter().take(MAX_THEMES) {
                if !matches(row, &words) {
                    continue;
                }
                let selected = !current.is_empty() && text(row.get("id")) == current;
                let group = if selected {
                    0
                } else if text(row.get("mode")) == "dark" {
                    1
                } else {
                    2
                };
                let dots = swatches(row);
                let mut row = row.clone();
                if let Some(object) = row.as_object_mut() {
                    object.insert("current".to_owned(), json!(selected));
                    object.insert(
                        "section".to_owned(),
                        json!(["CURRENT", "DARK", "LIGHT"][group]),
                    );
                    object.insert("swatches".to_owned(), dots);
                }
                groups[group].push(row);
            }
            let [selected, dark, light] = groups;
            // Which row opens a group is decided here, so a delegate never has
            // to look at the row above it to know whether to draw a heading.
            let mut listed: Vec<Value> = selected.into_iter().chain(dark).chain(light).collect();
            let mut opened = String::new();
            for row in &mut listed {
                let section = text(row.get("section"));
                let first = section != opened;
                opened = section;
                if let Some(object) = row.as_object_mut() {
                    object.insert("first".to_owned(), json!(first));
                }
            }
            json!(listed)
        }
        // The one line under the panel title: what is applied now, and how much
        // of the catalog the search has left.
        "detail" => {
            let themes = array(first.get("themes"));
            let current = text(first.get("current"));
            let shown = number(args.get(1)).max(0.0) as usize;
            let total = themes.len();
            let name = themes
                .iter()
                .find(|row| text(row.get("id")) == current)
                .map(|row| text(row.get("name")))
                .unwrap_or_default();
            let count = if shown == total {
                format!("{total} preset{}", if total == 1 { "" } else { "s" })
            } else {
                format!("{shown} of {total}")
            };
            json!(if name.is_empty() {
                count
            } else {
                format!("{name} · {count}")
            })
        }
        // The display name of one ID, so a tile or a heading names a theme
        // without a lookup loop in QML.
        "name" => {
            let id = text(args.get(1));
            json!(
                array(Some(first))
                    .iter()
                    .find(|row| text(row.get("id")) == id)
                    .map(|row| text(row.get("name")))
                    .unwrap_or_default()
            )
        }
        // The published selection, read for what a surface may display of it.
        // The file belongs to the helper, and is still reduced to an ID this
        // catalog could contain and a name that draws as one plain line.
        "selected" => {
            let id = text(first.get("id"));
            let name = text(first.get("name"));
            let named = !name.is_empty()
                && name.chars().count() <= 80
                && !name.chars().any(char::is_control);
            json!({
                "id": if identifier(&id) { id } else { String::new() },
                "name": if named { name } else { String::new() },
            })
        }
        "failure" => json!(failure(&text(Some(first)))),
        "pending" => json!(pending(array(Some(first)))),
        _ => return Err("unknown Themes UI function".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::string;

    fn preset(id: &str, name: &str, mode: &str) -> Value {
        let mut value = json!({"id": id, "name": name, "mode": mode});
        let object = value.as_object_mut().unwrap();
        for (index, role) in ROLES.iter().enumerate() {
            object.insert(
                (*role).to_owned(),
                json!(format!("#{:06x}", index * 0x111111)),
            );
        }
        value
    }
    fn catalog() -> Value {
        call(
            "catalog",
            &[json!({"current": "nord", "themes": [
                preset("catppuccin-mocha", "Catppuccin Mocha", "dark"),
                preset("nord", "Nord", "dark"),
                preset("rose-pine-dawn", "Rosé Pine Dawn", "light"),
                preset("gruvbox-light", "Gruvbox Light", "light"),
            ]})],
        )
        .unwrap()
    }
    fn rows(catalog: &Value, query: &str) -> Vec<Value> {
        call(
            "rows",
            &[
                catalog["themes"].clone(),
                catalog["current"].clone(),
                json!(query),
            ],
        )
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
    }

    #[test]
    fn a_catalog_is_accepted_whole_or_not_at_all() {
        let accepted = catalog();
        assert_eq!(accepted["ok"], json!(true));
        assert_eq!(accepted["current"], json!("nord"));
        assert_eq!(accepted["themes"].as_array().unwrap().len(), 4);
        assert_eq!(accepted["themes"][0]["modeLabel"], json!("Dark"));
        assert_eq!(accepted["themes"][2]["modeLabel"], json!("Light"));

        let mut short = preset("nord", "Nord", "dark");
        short.as_object_mut().unwrap().remove("accent");
        for broken in [
            json!({"current": "nord", "themes": []}),
            json!({"current": "nord", "themes": [short]}),
            json!({"current": "nord", "themes": [preset("nord", "Nord", "sepia")]}),
            json!({"current": "nord", "themes": [preset("Nord!", "Nord", "dark")]}),
            json!({"current": "nord", "themes": [preset("nord", "", "dark")]}),
            json!({"current": "nord", "themes": [preset("nord", "Nord", "dark"), preset("nord", "Nord Again", "dark")]}),
            json!({"current": "nord", "themes": vec![preset("nord", "Nord", "dark"); MAX_THEMES + 1]}),
        ] {
            assert_eq!(
                call("catalog", std::slice::from_ref(&broken)).unwrap()["ok"],
                json!(false),
                "{broken}"
            );
        }
        let mut wrong = preset("nord", "Nord", "dark");
        wrong
            .as_object_mut()
            .unwrap()
            .insert("base".to_owned(), json!("black"));
        assert_eq!(
            call("catalog", &[json!({"current": "nord", "themes": [wrong]})]).unwrap()["ok"],
            json!(false),
            "a colour Qt would accept but the helper never writes is refused"
        );
        assert_eq!(
            call(
                "catalog",
                &[json!({"current": "missing", "themes": [preset("nord", "Nord", "dark")]})]
            )
            .unwrap()["current"],
            json!(""),
            "an unknown selection marks nothing"
        );
    }

    #[test]
    fn the_selection_leads_and_the_catalogs_own_order_is_kept() {
        let catalog = catalog();
        let listed = rows(&catalog, "");
        assert_eq!(
            listed
                .iter()
                .map(|row| string(row.get("id")))
                .collect::<Vec<_>>(),
            [
                "nord",
                "catppuccin-mocha",
                "rose-pine-dawn",
                "gruvbox-light"
            ],
            "the applied theme leads, then dark and light in catalog order"
        );
        assert_eq!(
            listed
                .iter()
                .map(|row| string(row.get("section")))
                .collect::<Vec<_>>(),
            ["CURRENT", "DARK", "LIGHT", "LIGHT"]
        );
        assert_eq!(listed[0]["current"], json!(true));
        assert_eq!(listed[1]["current"], json!(false));
        assert_eq!(
            listed
                .iter()
                .map(|row| row["first"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            [true, true, true, false],
            "each group's first shown row carries its own heading"
        );
        let filtered = rows(&catalog, "light");
        assert_eq!(
            filtered
                .iter()
                .map(|row| row["first"].as_bool().unwrap())
                .collect::<Vec<_>>(),
            [true, false],
            "a heading belongs to the first row a search actually left in the group"
        );
        assert_eq!(
            listed[0]["swatches"],
            json!([
                listed[0]["accent"],
                listed[0]["red"],
                listed[0]["green"],
                listed[0]["yellow"]
            ]),
            "a row carries the colours its preview is drawn with"
        );
    }

    #[test]
    fn a_search_reaches_a_preset_by_family_variant_or_mode() {
        let catalog = catalog();
        for (query, expected) in [
            ("mocha", vec!["catppuccin-mocha"]),
            ("CATPPUCCIN", vec!["catppuccin-mocha"]),
            ("  rosé  ", vec!["rose-pine-dawn"]),
            ("rose dawn", vec!["rose-pine-dawn"]),
            ("light", vec!["rose-pine-dawn", "gruvbox-light"]),
            ("nothing", vec![]),
        ] {
            assert_eq!(
                rows(&catalog, query)
                    .iter()
                    .map(|row| string(row.get("id")))
                    .collect::<Vec<_>>(),
                expected,
                "{query}"
            );
        }
    }

    #[test]
    fn the_header_states_what_is_applied_and_how_much_is_shown() {
        let catalog = catalog();
        assert_eq!(
            string(Some(&call("detail", &[catalog.clone(), json!(4)]).unwrap())),
            "Nord · 4 presets"
        );
        assert_eq!(
            string(Some(&call("detail", &[catalog, json!(1)]).unwrap())),
            "Nord · 1 of 4"
        );
        let unselected = call(
            "catalog",
            &[json!({"current": "", "themes": [preset("nord", "Nord", "dark")]})],
        )
        .unwrap();
        assert_eq!(
            string(Some(&call("detail", &[unselected, json!(1)]).unwrap())),
            "1 preset"
        );
    }

    #[test]
    fn what_did_not_reload_is_named_without_claiming_a_failure() {
        assert_eq!(string(Some(&call("pending", &[json!([])]).unwrap())), "");
        assert_eq!(
            string(Some(&call("pending", &[json!(["tmux"])]).unwrap())),
            "tmux still shows the previous palette."
        );
        assert_eq!(
            string(Some(
                &call("pending", &[json!(["Ghostty", "tmux"])]).unwrap()
            )),
            "Ghostty and tmux still show the previous palette."
        );
        assert_eq!(
            string(Some(
                &call("pending", &[json!(["Window borders", "Ghostty", "tmux"])]).unwrap()
            )),
            "Window borders, Ghostty and tmux still show the previous palette."
        );
        assert_eq!(
            string(Some(
                &call("pending", &[json!(["tm\nux", "", "Ghostty"])]).unwrap()
            )),
            "tmux and Ghostty still show the previous palette.",
            "a name is drawn as one plain line"
        );
        let many = json!(vec![json!("tmux"); MAX_PENDING + 4]);
        assert!(
            string(Some(&call("pending", &[many]).unwrap()))
                .matches("tmux")
                .count()
                <= MAX_PENDING
        );
    }

    #[test]
    fn a_theme_is_named_from_the_catalog_or_from_the_published_selection() {
        let catalog = catalog();
        assert_eq!(
            string(Some(
                &call("name", &[catalog["themes"].clone(), json!("nord")]).unwrap()
            )),
            "Nord"
        );
        assert_eq!(
            string(Some(
                &call("name", &[catalog["themes"].clone(), json!("missing")]).unwrap()
            )),
            "",
            "an unknown ID is named as nothing rather than as another theme"
        );
        let selected = call(
            "selected",
            &[json!({"id": "catppuccin-mocha", "name": "Catppuccin Mocha", "wallpaper": "/x"})],
        )
        .unwrap();
        assert_eq!(selected["id"], json!("catppuccin-mocha"));
        assert_eq!(selected["name"], json!("Catppuccin Mocha"));
        for broken in [
            json!({"id": "Not An Id", "name": "Nord"}),
            json!({"name": "Nord"}),
            json!(null),
        ] {
            assert_eq!(
                call("selected", std::slice::from_ref(&broken)).unwrap()["id"],
                json!(""),
                "{broken}"
            );
        }
        assert_eq!(
            call("selected", &[json!({"id": "nord", "name": "No\nrd"})]).unwrap()["name"],
            json!(""),
            "a name that is not one plain line is not displayed"
        );
    }

    #[test]
    fn every_refusal_says_what_happened_to_the_selection() {
        for code in ["unavailable", "invalid", "unknown", "timeout", "nonsense"] {
            let text = string(Some(&call("failure", &[json!(code)]).unwrap()));
            assert!(!text.is_empty(), "{code} needs wording");
            assert!(
                !text.to_lowercase().contains("applied successfully"),
                "{code} must not read as a success"
            );
        }
        assert_eq!(string(Some(&call("failure", &[json!("")]).unwrap())), "");
        assert!(call("nonsense", &[]).is_err());
    }
}
