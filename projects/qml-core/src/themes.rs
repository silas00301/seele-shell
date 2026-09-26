//! Theme picker presentation: one normalized catalog, the carousel a picker
//! steps through, what the schedule will do next, and the wording for what a
//! switch did and did not reach.
//!
//! Publication, reloading, the light and dark slots and the schedule itself
//! belong to `seele-theme`. What is left here is what one open picker is
//! looking at, and it is kept in Rust so a palette never reaches a Qt colour
//! property without having been checked, and so ordering, filtering and the
//! schedule's sentence are decided once rather than by a delegate.
use crate::value::{array, text, trim};
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

/// A carousel entry's second line: its mode, unless its name already says it
/// ("Flexoki Light").
fn detail(row: &Value) -> String {
    let mode = text(row.get("mode"));
    let named = text(row.get("name"))
        .split_whitespace()
        .any(|word| word.eq_ignore_ascii_case(&mode));
    if named {
        String::new()
    } else {
        text(row.get("modeLabel"))
    }
}

/// One line saying what the schedule will do next, from the helper's own
/// description of it, or nothing while it is off.
fn schedule(appearance: &Value) -> String {
    let auto = appearance.get("auto").unwrap_or(&Value::Null);
    let source = text(auto.get("source"));
    let place = text(appearance.get("place"));
    let next = appearance.get("next").unwrap_or(&Value::Null);
    let when = |value: &Value| {
        let mode = mode_label(&text(value.get("mode")));
        format!("{mode} at {}", text(value.get("clock")))
    };
    match source.as_str() {
        "schedule" if !next.is_null() => format!("{} · on a schedule", when(next)),
        "sun" if place.is_empty() => "Sunrise and sunset need a timezone with a city".to_owned(),
        "sun" if next.is_null() => {
            let polar = text(appearance.get("sun").and_then(|sun| sun.get("polar")));
            if polar == "day" {
                format!("The sun does not set in {place} today")
            } else {
                format!("The sun does not rise in {place} today")
            }
        }
        "sun" => {
            let event = if text(next.get("mode")) == "light" {
                "sunrise"
            } else {
                "sunset"
            };
            format!("{} · {event} in {place}", when(next))
        }
        _ => String::new(),
    }
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
        // The strip a picker steps through: the catalog's own curated order,
        // narrowed to the mode being edited unless the reader asked for all,
        // and by the search. Each entry says whether it is the slot's own.
        "carousel" => {
            let themes = array(Some(first));
            let slot = text(args.get(1));
            let query = text(args.get(2)).to_lowercase();
            let scope = text(args.get(3));
            let words: Vec<String> = trim(&query)
                .split_whitespace()
                .take(MAX_WORDS)
                .map(str::to_owned)
                .collect();
            let in_scope = |row: &Value| {
                !matches!(scope.as_str(), "dark" | "light") || text(row.get("mode")) == scope
            };
            let items: Vec<Value> = themes
                .iter()
                .take(MAX_THEMES)
                .filter(|row| in_scope(row) && matches(row, &words))
                .map(|row| {
                    let mut row = row.clone();
                    let id = text(row.get("id"));
                    let line = detail(&row);
                    if let Some(object) = row.as_object_mut() {
                        object.insert("detail".to_owned(), json!(line));
                        object.insert("current".to_owned(), json!(!slot.is_empty() && id == slot));
                    }
                    row
                })
                .collect();
            let order: Vec<String> = items.iter().map(|row| text(row.get("id"))).collect();
            json!({
                "items": items,
                "order": order,
                "count": order.len(),
                // How many the scope alone leaves, so a picker can say how
                // much "all" would add.
                "scoped": themes.iter().take(MAX_THEMES).filter(|row| in_scope(row)).count(),
                "total": themes.len(),
            })
        }
        // Where left and right lead from an entry. The ends hold rather than
        // wrap, so a held key stops somewhere the reader can see.
        "step" => {
            let order: Vec<String> = array(first.get("order"))
                .iter()
                .map(|id| text(Some(id)))
                .collect();
            let id = text(args.get(1));
            let direction = text(args.get(2));
            let Some(at) = order.iter().position(|entry| *entry == id) else {
                return Ok(json!(order.first().cloned().unwrap_or_default()));
            };
            let target = match direction.as_str() {
                "left" | "previous" => at.saturating_sub(1),
                "right" | "next" => (at + 1).min(order.len() - 1),
                _ => at,
            };
            json!(order[target])
        }
        "schedule" => json!(schedule(first)),
        // Which entry the carousel centres: the highlighted preset while the
        // search still shows it, else the slot's own, else the first shown.
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
    fn carousel(catalog: &Value, slot: &str, query: &str, scope: &str) -> Value {
        call(
            "carousel",
            &[
                catalog["themes"].clone(),
                json!(slot),
                json!(query),
                json!(scope),
            ],
        )
        .unwrap()
    }
    fn names(carousel: &Value) -> Vec<String> {
        array(carousel.get("items"))
            .iter()
            .map(|item| string(item.get("name")))
            .collect()
    }
    fn step(carousel: &Value, id: &str, direction: &str) -> String {
        string(Some(
            &call("step", &[carousel.clone(), json!(id), json!(direction)]).unwrap(),
        ))
    }

    #[test]
    fn the_carousel_shows_the_edited_modes_presets_unless_asked_for_all() {
        let catalog = curated();
        let light = carousel(&catalog, "catppuccin-latte", "", "light");
        assert_eq!(
            names(&light),
            [
                "Catppuccin Latte",
                "Rosé Pine Dawn",
                "Flexoki Light",
                "Gruvbox Light"
            ],
            "the catalog's own order, narrowed to the mode being edited"
        );
        assert_eq!(light["count"], json!(4));
        assert_eq!(light["scoped"], json!(4));
        assert_eq!(light["total"], json!(13));
        assert_eq!(
            light["items"][0]["current"],
            json!(true),
            "the slot's own preset is marked"
        );
        assert_eq!(light["items"][1]["current"], json!(false));
        let all = carousel(&catalog, "catppuccin-latte", "", "all");
        assert_eq!(all["count"], json!(13), "every preset may fill either slot");
        assert_eq!(carousel(&catalog, "nord", "", "dark")["count"], json!(9));
    }

    #[test]
    fn a_search_narrows_whatever_the_scope_leaves() {
        let catalog = curated();
        assert_eq!(
            names(&carousel(&catalog, "", "rose", "all")),
            ["Rosé Pine", "Rosé Pine Moon", "Rosé Pine Dawn"]
        );
        assert_eq!(
            names(&carousel(&catalog, "", "ROSÉ dawn", "all")),
            ["Rosé Pine Dawn"],
            "every word, in any case"
        );
        assert_eq!(
            names(&carousel(&catalog, "", "rose", "dark")),
            ["Rosé Pine", "Rosé Pine Moon"]
        );
        let nothing = carousel(&catalog, "", "solarized", "all");
        assert_eq!(nothing["count"], json!(0));
        assert_eq!(nothing["total"], json!(13));
    }

    #[test]
    fn an_entrys_second_line_names_its_mode_only_when_its_name_does_not() {
        let catalog = curated();
        let all = carousel(&catalog, "", "", "all");
        let detail = |name: &str| {
            array(all.get("items"))
                .iter()
                .find(|item| string(item.get("name")) == name)
                .map(|item| string(item.get("detail")))
                .unwrap()
        };
        assert_eq!(detail("Catppuccin Mocha"), "Dark");
        assert_eq!(detail("Rosé Pine Dawn"), "Light");
        assert_eq!(
            detail("Flexoki Light"),
            "",
            "Flexoki's Light already says so"
        );
        assert_eq!(detail("Gruvbox Dark"), "");
    }

    #[test]
    fn left_and_right_move_through_the_strip_and_hold_at_its_ends() {
        let light = carousel(&curated(), "", "", "light");
        assert_eq!(step(&light, "catppuccin-latte", "right"), "rose-pine-dawn");
        assert_eq!(step(&light, "rose-pine-dawn", "next"), "flexoki-light");
        assert_eq!(step(&light, "rose-pine-dawn", "left"), "catppuccin-latte");
        assert_eq!(step(&light, "catppuccin-latte", "left"), "catppuccin-latte");
        assert_eq!(
            step(&light, "gruvbox-light-medium", "right"),
            "gruvbox-light-medium"
        );
        assert_eq!(
            step(&light, "gone", "right"),
            "catppuccin-latte",
            "something no longer shown starts at the front"
        );
        assert_eq!(
            step(&carousel(&curated(), "", "zzz", "all"), "nord", "right"),
            ""
        );
    }

    #[test]
    fn the_centre_follows_the_highlight_then_the_slots_own_preset() {
        let catalog = curated();
        let all = carousel(&catalog, "catppuccin-mocha", "", "all");
        let focus = |carousel: &Value, highlighted: &str, slot: &str| {
            string(Some(
                &call(
                    "focus",
                    &[carousel.clone(), json!(highlighted), json!(slot)],
                )
                .unwrap(),
            ))
        };
        assert_eq!(focus(&all, "nord", "catppuccin-mocha"), "nord");
        assert_eq!(focus(&all, "", "catppuccin-mocha"), "catppuccin-mocha");
        let light = carousel(&catalog, "catppuccin-mocha", "", "light");
        assert_eq!(
            focus(&light, "nord", "catppuccin-mocha"),
            "catppuccin-latte",
            "a centre the scope hides falls back to the first entry shown"
        );
        assert_eq!(
            focus(&carousel(&catalog, "", "zzz", "all"), "nord", "nord"),
            ""
        );
    }

    #[test]
    fn the_schedule_says_what_it_will_do_next() {
        let say = |appearance: Value| string(Some(&call("schedule", &[appearance]).unwrap()));
        assert_eq!(say(json!({"auto": {"source": "off"}, "next": null})), "");
        assert_eq!(
            say(
                json!({"auto": {"source": "schedule"}, "next": {"mode": "dark", "clock": "19:00"}})
            ),
            "Dark at 19:00 · on a schedule"
        );
        assert_eq!(
            say(
                json!({"auto": {"source": "sun"}, "place": "Berlin", "next": {"mode": "light", "clock": "07:12"}})
            ),
            "Light at 07:12 · sunrise in Berlin"
        );
        assert_eq!(
            say(
                json!({"auto": {"source": "sun"}, "place": "Berlin", "next": {"mode": "dark", "clock": "16:02"}})
            ),
            "Dark at 16:02 · sunset in Berlin"
        );
        assert_eq!(
            say(
                json!({"auto": {"source": "sun"}, "place": "Tromsø", "next": null, "sun": {"polar": "night"}})
            ),
            "The sun does not rise in Tromsø today"
        );
        assert_eq!(
            say(json!({"auto": {"source": "sun"}, "place": null, "next": null})),
            "Sunrise and sunset need a timezone with a city"
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
