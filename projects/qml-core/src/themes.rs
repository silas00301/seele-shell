//! Theme picker presentation: one normalized catalog, the carousel the
//! switcher steps through, what the schedule will do next, and the wording for
//! what a switch did and did not reach.
//!
//! Publication, reloading, the light and dark slots and the schedule itself
//! belong to `seele-theme`. What is left here is what the Themes surfaces are
//! looking at, and it is kept in Rust so a palette never reaches a Qt colour
//! property without having been checked, and so ordering, movement and the
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
        // The strip the switcher steps through: every preset, in the catalog's
        // own curated order, each saying whether it is the one on screen.
        "carousel" => {
            let current = text(args.get(1));
            let items: Vec<Value> = array(Some(first))
                .iter()
                .take(MAX_THEMES)
                .map(|row| {
                    let mut row = row.clone();
                    let id = text(row.get("id"));
                    if let Some(object) = row.as_object_mut() {
                        object.insert(
                            "current".to_owned(),
                            json!(!current.is_empty() && id == current),
                        );
                    }
                    row
                })
                .collect();
            let order: Vec<String> = items.iter().map(|row| text(row.get("id"))).collect();
            json!({"items": items, "order": order, "count": order.len()})
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
        // Which entry the carousel centres: the preset just moved to, else
        // the one on screen, else the first.
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
    fn carousel(catalog: &Value, current: &str) -> Value {
        call("carousel", &[catalog["themes"].clone(), json!(current)]).unwrap()
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
    fn the_carousel_shows_every_preset_in_the_catalogs_order() {
        let catalog = curated();
        let all = carousel(&catalog, "catppuccin-latte");
        assert_eq!(all["count"], json!(13), "light and dark side by side");
        assert_eq!(
            &names(&all)[..5],
            [
                "Catppuccin Mocha",
                "Catppuccin Macchiato",
                "Catppuccin Frappé",
                "Catppuccin Latte",
                "Rosé Pine"
            ],
            "the catalog's own order, families together"
        );
        assert_eq!(
            all["items"][3]["current"],
            json!(true),
            "the preset on screen is marked"
        );
        assert_eq!(all["items"][0]["current"], json!(false));
        assert!(
            array(carousel(&catalog, "").get("items"))
                .iter()
                .all(|item| item["current"] == json!(false)),
            "nothing on screen marks nothing"
        );
    }

    #[test]
    fn left_and_right_move_through_the_strip_and_hold_at_its_ends() {
        let all = carousel(&curated(), "");
        assert_eq!(step(&all, "catppuccin-frappe", "right"), "catppuccin-latte");
        assert_eq!(step(&all, "catppuccin-latte", "next"), "rose-pine");
        assert_eq!(step(&all, "catppuccin-latte", "left"), "catppuccin-frappe");
        assert_eq!(step(&all, "catppuccin-mocha", "left"), "catppuccin-mocha");
        assert_eq!(
            step(&all, "everforest-dark-medium", "right"),
            "everforest-dark-medium"
        );
        assert_eq!(
            step(&all, "gone", "right"),
            "catppuccin-mocha",
            "something no longer in the catalog starts at the front"
        );
        let empty = call("carousel", &[json!([]), json!("")]).unwrap();
        assert_eq!(step(&empty, "nord", "right"), "");
    }

    #[test]
    fn the_centre_follows_the_move_then_the_preset_on_screen() {
        let catalog = curated();
        let all = carousel(&catalog, "catppuccin-mocha");
        let focus = |carousel: &Value, highlighted: &str, current: &str| {
            string(Some(
                &call(
                    "focus",
                    &[carousel.clone(), json!(highlighted), json!(current)],
                )
                .unwrap(),
            ))
        };
        assert_eq!(focus(&all, "nord", "catppuccin-mocha"), "nord");
        assert_eq!(focus(&all, "", "catppuccin-mocha"), "catppuccin-mocha");
        assert_eq!(
            focus(&all, "gone", "also-gone"),
            "catppuccin-mocha",
            "presets no longer in the catalog fall back to the first"
        );
        let empty = call("carousel", &[json!([]), json!("")]).unwrap();
        assert_eq!(focus(&empty, "nord", "nord"), "");
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
