//! Theme picker presentation: one normalized catalog, the families a panel
//! lays out, where the arrow keys lead, and the wording for what a switch did
//! and did not reach.
//!
//! Publication, reloading and every file the switch writes belong to
//! `seele-theme`. What is left here is what one open panel is looking at, and
//! it is kept in Rust so a palette never reaches a Qt colour property without
//! having been checked, and so grouping and movement are decided once rather
//! than by whichever delegate happens to be drawing.
use crate::value::{array, number, text, trim};
use serde_json::{Value, json};
use std::collections::BTreeMap;

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
const MAX_COLUMNS: usize = 8;

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

/// A family is the presets sharing a first word, labelled by the words all of
/// them share: "Catppuccin" for Mocha to Latte, "Rosé Pine" for the original,
/// Moon and Dawn. A variant is what its own name adds; the original of a family
/// adds nothing and keeps its full name. Presets alone in their family are
/// gathered into one closing row, so a single preset never gets a row whose
/// label repeats its only tile.
fn families(themes: &[Value]) -> Vec<(String, Vec<(Value, String)>)> {
    let mut order: Vec<String> = vec![];
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for row in themes {
        let name = text(row.get("name"));
        let key = name
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row);
    }
    let mut rows = vec![];
    let mut alone = vec![];
    for key in order {
        let members = &groups[&key];
        if members.len() == 1 {
            let row = members[0];
            alone.push((row.clone(), text(row.get("name"))));
            continue;
        }
        let names: Vec<Vec<String>> = members
            .iter()
            .map(|row| {
                text(row.get("name"))
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect()
            })
            .collect();
        let shared = (0..names.iter().map(Vec::len).min().unwrap_or(0))
            .take_while(|&index| names.iter().all(|words| words[index] == names[0][index]))
            .count()
            .max(1);
        let family = names[0][..shared].join(" ");
        let variants = members
            .iter()
            .zip(&names)
            .map(|(row, words)| {
                let variant = words[shared.min(words.len())..].join(" ");
                let label = if variant.is_empty() {
                    text(row.get("name"))
                } else {
                    variant
                };
                ((*row).clone(), label)
            })
            .collect();
        rows.push((family, variants));
    }
    if !alone.is_empty() {
        let label = if rows.is_empty() { "Presets" } else { "More" };
        rows.push((label.to_owned(), alone));
    }
    rows
}

/// Where each shown preset sits: `(row, column)` in reading order.
fn positions(layout: &Value) -> Vec<(usize, usize, String)> {
    let mut out = vec![];
    for (row, entry) in array(layout.get("rows")).iter().enumerate() {
        for (column, member) in array(entry.get("members")).iter().enumerate() {
            out.push((row, column, text(member.get("id"))));
        }
    }
    out
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
        // The families a panel lays out, filtered by the search and the mode,
        // each chunked into rows of at most `columns` tiles. A family keeps
        // the catalog's own curated order inside it; the label rides on its
        // first row only, so a family that wraps still reads as one.
        "layout" => {
            let themes = array(Some(first));
            let current = text(args.get(1));
            let query = text(args.get(2)).to_lowercase();
            let mode = text(args.get(3));
            let columns = (number(args.get(4)).max(1.0) as usize).min(MAX_COLUMNS);
            let words: Vec<String> = trim(&query)
                .split_whitespace()
                .take(MAX_WORDS)
                .map(str::to_owned)
                .collect();
            // Families come from the whole catalog, and the search and mode
            // only take tiles out of them: a tile keeps its label and its row
            // while the reader types, instead of the grid regrouping itself.
            let catalog: Vec<Value> = themes.iter().take(MAX_THEMES).cloned().collect();
            let shown = |row: &Value| {
                (!matches!(mode.as_str(), "dark" | "light") || text(row.get("mode")) == mode)
                    && matches(row, &words)
            };
            let mut rows = vec![];
            let mut order = vec![];
            for (family, members) in families(&catalog) {
                let members: Vec<&(Value, String)> =
                    members.iter().filter(|(row, _)| shown(row)).collect();
                for (index, chunk) in members.chunks(columns).enumerate() {
                    let members: Vec<Value> = chunk
                        .iter()
                        .map(|(row, variant)| {
                            let mut row = row.clone();
                            let id = text(row.get("id"));
                            order.push(id.clone());
                            if let Some(object) = row.as_object_mut() {
                                object.insert("variant".to_owned(), json!(variant));
                                object.insert(
                                    "current".to_owned(),
                                    json!(!current.is_empty() && id == current),
                                );
                            }
                            row
                        })
                        .collect();
                    rows.push(json!({
                        "family": if index == 0 { family.clone() } else { String::new() },
                        "first": index == 0,
                        "members": members,
                    }));
                }
            }
            json!({"rows": rows, "order": order, "count": order.len(), "total": themes.len()})
        }
        // Where an arrow key leads from a tile. Left and right follow reading
        // order across rows; up and down keep the column where the next row
        // is long enough to have it. The ends hold rather than wrap, so a held
        // key stops somewhere the reader can see.
        "step" => {
            let places = positions(first);
            let id = text(args.get(1));
            let direction = text(args.get(2));
            let Some(at) = places.iter().position(|place| place.2 == id) else {
                return Ok(json!(
                    places
                        .first()
                        .map(|place| place.2.clone())
                        .unwrap_or_default()
                ));
            };
            let (row, column, _) = places[at];
            let target = match direction.as_str() {
                "left" => at.checked_sub(1),
                "right" => (at + 1 < places.len()).then_some(at + 1),
                "up" | "down" => {
                    let rows = places.last().map_or(0, |place| place.0 + 1);
                    let next = if direction == "up" {
                        row.checked_sub(1)
                    } else {
                        (row + 1 < rows).then_some(row + 1)
                    };
                    next.and_then(|next| {
                        places
                            .iter()
                            .enumerate()
                            .filter(|(_, place)| place.0 == next)
                            .take_while(|(_, place)| place.1 <= column)
                            .last()
                            .map(|(index, _)| index)
                    })
                }
                _ => None,
            };
            json!(places[target.unwrap_or(at)].2)
        }
        // What the preview shows: the highlighted preset while the search
        // still shows it, else the applied theme, else the first shown one.
        "focus" => {
            let order: Vec<String> = array(first.get("order"))
                .iter()
                .map(|id| text(Some(id)))
                .collect();
            let highlighted = text(args.get(1));
            let current = text(args.get(2));
            json!(
                [highlighted, current]
                    .into_iter()
                    .find(|id| !id.is_empty() && order.contains(id))
                    .or_else(|| order.first().cloned())
                    .unwrap_or_default()
            )
        }
        // One preset by ID, for the preview; `null` when it is not in the
        // catalog rather than whichever preset happens to come first.
        "find" => {
            let id = text(args.get(1));
            array(Some(first))
                .iter()
                .find(|row| text(row.get("id")) == id)
                .cloned()
                .unwrap_or(Value::Null)
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
    // The curated catalog, in its own order, as the parent flake declares it.
    fn curated() -> Value {
        let presets = [
            ("catppuccin-mocha", "Catppuccin Mocha", "dark"),
            ("catppuccin-macchiato", "Catppuccin Macchiato", "dark"),
            ("catppuccin-frappe", "Catppuccin Frappé", "dark"),
            ("catppuccin-latte", "Catppuccin Latte", "light"),
            ("rose-pine", "Rosé Pine", "dark"),
            ("rose-pine-moon", "Rosé Pine Moon", "dark"),
            ("rose-pine-dawn", "Rosé Pine Dawn", "light"),
            ("flexoki-dark", "Flexoki Dark", "dark"),
            ("flexoki-light", "Flexoki Light", "light"),
            ("gruvbox-dark-medium", "Gruvbox Dark", "dark"),
            ("gruvbox-light-medium", "Gruvbox Light", "light"),
            ("nord", "Nord", "dark"),
            ("everforest-dark-medium", "Everforest", "dark"),
        ];
        let themes: Vec<Value> = presets
            .iter()
            .map(|(id, name, mode)| preset(id, name, mode))
            .collect();
        call(
            "catalog",
            &[json!({"current": "catppuccin-mocha", "themes": themes})],
        )
        .unwrap()
    }
    fn layout(catalog: &Value, query: &str, mode: &str, columns: u64) -> Value {
        call(
            "layout",
            &[
                catalog["themes"].clone(),
                catalog["current"].clone(),
                json!(query),
                json!(mode),
                json!(columns),
            ],
        )
        .unwrap()
    }
    // Each row as "Family: variant, variant", so a whole layout reads at once.
    fn shape(layout: &Value) -> Vec<String> {
        array(layout.get("rows"))
            .iter()
            .map(|row| {
                let tiles: Vec<String> = array(row.get("members"))
                    .iter()
                    .map(|member| string(member.get("variant")))
                    .collect();
                format!("{}: {}", string(row.get("family")), tiles.join(", "))
            })
            .collect()
    }
    fn step(layout: &Value, id: &str, direction: &str) -> String {
        string(Some(
            &call("step", &[layout.clone(), json!(id), json!(direction)]).unwrap(),
        ))
    }

    #[test]
    fn presets_group_into_families_named_by_what_they_share() {
        let listed = layout(&curated(), "", "all", 4);
        assert_eq!(
            shape(&listed),
            [
                "Catppuccin: Mocha, Macchiato, Frappé, Latte",
                "Rosé Pine: Rosé Pine, Moon, Dawn",
                "Flexoki: Dark, Light",
                "Gruvbox: Dark, Light",
                "More: Nord, Everforest",
            ],
            "an original keeps its full name, and presets alone in a family share one closing row"
        );
        assert_eq!(listed["count"], json!(13));
        assert_eq!(listed["total"], json!(13));
        let mocha = &listed["rows"][0]["members"][0];
        assert_eq!(mocha["current"], json!(true));
        assert_eq!(
            mocha["name"],
            json!("Catppuccin Mocha"),
            "the full name travels with the tile"
        );
        assert_eq!(listed["rows"][1]["members"][0]["current"], json!(false));
        assert_eq!(
            array(listed.get("order")).len(),
            13,
            "every shown preset has one place in reading order"
        );
    }

    #[test]
    fn the_search_and_the_mode_narrow_the_families_they_leave() {
        let catalog = curated();
        assert_eq!(
            shape(&layout(&catalog, "", "light", 4)),
            [
                "Catppuccin: Latte",
                "Rosé Pine: Dawn",
                "Flexoki: Light",
                "Gruvbox: Light",
            ]
        );
        assert_eq!(
            shape(&layout(&catalog, "pine", "dark", 4)),
            ["Rosé Pine: Rosé Pine, Moon"]
        );
        assert_eq!(
            shape(&layout(&catalog, "ROSÉ dawn", "all", 4)),
            ["Rosé Pine: Dawn"],
            "every word has to match, in any case"
        );
        let nothing = layout(&catalog, "solarized", "all", 4);
        assert_eq!(shape(&nothing), Vec::<String>::new());
        assert_eq!(nothing["count"], json!(0));
        assert_eq!(nothing["total"], json!(13));
    }

    #[test]
    fn a_family_wider_than_the_panel_wraps_but_is_labelled_once() {
        let listed = layout(&curated(), "catppuccin", "all", 3);
        assert_eq!(
            shape(&listed),
            ["Catppuccin: Mocha, Macchiato, Frappé", ": Latte"]
        );
        assert_eq!(listed["rows"][0]["first"], json!(true));
        assert_eq!(listed["rows"][1]["first"], json!(false));
        assert_eq!(
            shape(&layout(&curated(), "catppuccin", "all", 0)).len(),
            4,
            "a nonsensical column count still lays out one tile per row"
        );
    }

    #[test]
    fn arrow_keys_move_in_reading_order_and_keep_their_column() {
        let listed = layout(&curated(), "", "all", 4);
        // Across a row, and over its end into the next row.
        assert_eq!(
            step(&listed, "catppuccin-mocha", "right"),
            "catppuccin-macchiato"
        );
        assert_eq!(step(&listed, "catppuccin-latte", "right"), "rose-pine");
        assert_eq!(step(&listed, "rose-pine", "left"), "catppuccin-latte");
        // Down keeps the column where the next row has it, and otherwise
        // lands on that row's last tile rather than skipping it.
        assert_eq!(
            step(&listed, "catppuccin-macchiato", "down"),
            "rose-pine-moon"
        );
        assert_eq!(step(&listed, "catppuccin-latte", "down"), "rose-pine-dawn");
        assert_eq!(step(&listed, "rose-pine-dawn", "down"), "flexoki-light");
        assert_eq!(step(&listed, "flexoki-light", "up"), "rose-pine-moon");
        // The ends hold.
        assert_eq!(
            step(&listed, "catppuccin-mocha", "left"),
            "catppuccin-mocha"
        );
        assert_eq!(
            step(&listed, "catppuccin-frappe", "up"),
            "catppuccin-frappe"
        );
        assert_eq!(
            step(&listed, "everforest-dark-medium", "right"),
            "everforest-dark-medium"
        );
        assert_eq!(step(&listed, "nord", "down"), "nord");
        // Something the layout no longer holds starts again at the top.
        assert_eq!(step(&listed, "gone", "down"), "catppuccin-mocha");
        assert_eq!(
            step(&layout(&curated(), "solarized", "all", 4), "nord", "down"),
            ""
        );
    }

    #[test]
    fn the_preview_follows_the_highlight_then_the_applied_theme() {
        let catalog = curated();
        let all = layout(&catalog, "", "all", 4);
        let focus = |layout: &Value, highlighted: &str, current: &str| {
            string(Some(
                &call(
                    "focus",
                    &[layout.clone(), json!(highlighted), json!(current)],
                )
                .unwrap(),
            ))
        };
        assert_eq!(focus(&all, "nord", "catppuccin-mocha"), "nord");
        assert_eq!(focus(&all, "", "catppuccin-mocha"), "catppuccin-mocha");
        let light = layout(&catalog, "", "light", 4);
        assert_eq!(
            focus(&light, "nord", "catppuccin-mocha"),
            "catppuccin-latte",
            "a filtered-out highlight falls back to the first preset still shown"
        );
        assert_eq!(
            focus(&layout(&catalog, "zzz", "all", 4), "nord", "nord"),
            ""
        );
        let found = call("find", &[catalog["themes"].clone(), json!("nord")]).unwrap();
        assert_eq!(found["name"], json!("Nord"));
        assert!(
            call("find", &[catalog["themes"].clone(), json!("gone")])
                .unwrap()
                .is_null()
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
