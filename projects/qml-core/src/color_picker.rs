//! What a sampled pixel is called, and what pressing Enter will put on the
//! clipboard. The worker answers three bytes; everything a reader is told about
//! those bytes is decided here, once, so the overlay and the result card cannot
//! disagree about the same colour.
use crate::value::{array, finite, fixed, text};
use serde_json::{json, Value};

/// The shell's palette is eleven named roles, and a surface drawn from them is
/// almost always a tint of one composited over the wallpaper. So an exact match
/// is worth saying plainly — that pixel *is* `accent` — while a near miss is
/// only worth reporting with the distance attached. Beyond this the nearest
/// entry stops being informative and naming it would invite the reader to write
/// down a token that is not the colour they sampled.
const NEAR: f64 = 10.0;

/// Every colour has a hex form, so it is both the default and the fallback.
const HEX: &str = "hex";
const RGB: &str = "rgb";
const TOKEN: &str = "token";

fn channel(value: Option<&Value>) -> u8 {
    finite(value, 0.0).round().clamp(0.0, 255.0) as u8
}

/// Accepts the forms Qt writes a colour in. `#AARRGGBB` is deliberately
/// rejected unless it is fully opaque: a translucent token is composited over
/// whatever is behind it, so a screen pixel can never be that token, and
/// matching against its opaque channels would name the wrong thing.
fn parse(value: &str) -> Option<[u8; 3]> {
    let digits = value.trim().strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    match digits.len() {
        3 => {
            let mut rgb = [0u8; 3];
            for (index, value) in rgb.iter_mut().enumerate() {
                let nibble = u8::from_str_radix(&digits[index..index + 1], 16).ok()?;
                *value = nibble * 17;
            }
            Some(rgb)
        }
        6 => Some([byte(0)?, byte(2)?, byte(4)?]),
        8 => {
            if byte(0)? != 255 {
                return None;
            }
            Some([byte(2)?, byte(4)?, byte(6)?])
        }
        _ => None,
    }
}

fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

/// CIELAB through the sRGB transfer function and a D65 white point. The
/// distance below is CIE76, which is a plain Euclidean distance in that space:
/// it is not perceptually even everywhere — it over-penalizes saturated blues —
/// but it is a number whose derivation fits on screen, and it is only ever used
/// to decide whether a palette entry is close enough to be worth naming.
fn lab(rgb: [u8; 3]) -> [f64; 3] {
    let linear = |value: u8| {
        let value = f64::from(value) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let (red, green, blue) = (linear(rgb[0]), linear(rgb[1]), linear(rgb[2]));
    let x = (0.412_456_4 * red + 0.357_576_1 * green + 0.180_437_5 * blue) / 0.950_47;
    let y = 0.212_672_9 * red + 0.715_152_2 * green + 0.072_175_0 * blue;
    let z = (0.019_333_9 * red + 0.119_192_0 * green + 0.950_304_1 * blue) / 1.088_83;
    let f = |value: f64| {
        if value > 216.0 / 24389.0 {
            value.cbrt()
        } else {
            value * 841.0 / 108.0 + 4.0 / 29.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn difference(a: [u8; 3], b: [u8; 3]) -> f64 {
    let (a, b) = (lab(a), lab(b));
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// The nearest palette entry, its distance, and whether the pixel simply is it.
fn nearest(rgb: [u8; 3], palette: Option<&Value>) -> (String, bool, f64) {
    let Some(Value::Object(palette)) = palette else {
        return (String::new(), false, f64::INFINITY);
    };
    let mut name = String::new();
    let mut distance = f64::INFINITY;
    for (key, value) in palette {
        let Some(candidate) = value.as_str().and_then(parse) else {
            continue;
        };
        if candidate == rgb {
            return (key.clone(), true, 0.0);
        }
        let apart = difference(rgb, candidate);
        // A tie keeps the name seen first, so the same pixel is always
        // reported with the same token rather than one of two.
        if apart < distance {
            name = key.clone();
            distance = apart;
        }
    }
    (name, false, distance)
}

fn entry(rgb: [u8; 3], palette: Option<&Value>) -> Value {
    let (name, exact, distance) = nearest(rgb, palette);
    let named = exact || distance <= NEAR;
    json!({
        "hex": hex(rgb),
        "rgb": format!("rgb({}, {}, {})", rgb[0], rgb[1], rgb[2]),
        "r": rgb[0], "g": rgb[1], "b": rgb[2],
        "token": if named { name.clone() } else { String::new() },
        "exact": exact,
        "note": if exact {
            name
        } else if named {
            let apart = fixed(distance, 1);
            format!("nearest {name} · ΔE {apart}")
        } else {
            String::new()
        },
    })
}

fn available(entry: Option<&Value>) -> Vec<&'static str> {
    let exact = entry
        .and_then(|entry| entry.get("exact"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let named = !text(entry.and_then(|entry| entry.get("token"))).is_empty();
    if exact && named {
        vec![HEX, RGB, TOKEN]
    } else {
        vec![HEX, RGB]
    }
}

pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    Ok(match function {
        "describe" => {
            let sample = args.first();
            entry(
                [
                    channel(sample.and_then(|value| value.get("r"))),
                    channel(sample.and_then(|value| value.get("g"))),
                    channel(sample.and_then(|value| value.get("b"))),
                ],
                args.get(1),
            )
        }
        // The format is an exclusive choice over what this colour actually
        // offers, so a colour that is not exactly a token cannot be left
        // holding the token format from the colour before it.
        "cycle" => {
            let formats = available(args.first());
            let current = text(args.get(1));
            let at = formats.iter().position(|format| *format == current);
            json!(formats[at.map_or(0, |at| (at + 1) % formats.len())])
        }
        // Resolves what was asked for against what this colour has, rather than
        // refusing: recalling an older pick should copy it, and the surface
        // reports the format that was actually used.
        "payload" => {
            let entry = args.first();
            let wanted = text(args.get(1));
            let format = if available(entry).contains(&wanted.as_str()) {
                wanted
            } else {
                HEX.to_owned()
            };
            let value = text(entry.and_then(|entry| entry.get(format.as_str())));
            if value.is_empty() {
                return Err("colour has no copyable text".into());
            }
            json!({ "text": value, "format": format })
        }
        // Session memory only, and short enough that every entry keeps a single
        // digit. Re-picking a colour moves it back to the front instead of
        // spending a second slot saying the same thing.
        "history" => {
            let addition = args.get(1).cloned().unwrap_or(Value::Null);
            let hex = text(addition.get("hex"));
            if hex.is_empty() {
                return Err("colour has no hex form".into());
            }
            let limit = finite(args.get(2), 9.0).clamp(1.0, 64.0) as usize;
            let mut entries = vec![addition];
            entries.extend(
                array(args.first())
                    .iter()
                    .filter(|entry| text(entry.get("hex")) != hex)
                    .take(limit - 1)
                    .cloned(),
            );
            Value::Array(entries)
        }
        // The lens shows source pixels, not a smoothed blow-up, so its window
        // is stated in whole pixels and stays that size against an edge by
        // sliding rather than by shrinking. `cx`/`cy` locate the sampled pixel
        // inside that window, which is where the crosshair goes.
        "loupe" => {
            let width = finite(args.get(2), 0.0).max(0.0) as u64;
            let height = finite(args.get(3), 0.0).max(0.0) as u64;
            if width == 0 || height == 0 {
                return Err("frame has no pixels".into());
            }
            let cells = finite(args.get(4), 9.0).clamp(1.0, 4096.0) as u64;
            let point = |value: Option<&Value>, size: u64| {
                (finite(value, 0.0).max(0.0) * size as f64).floor().min((size - 1) as f64) as u64
            };
            let column = point(args.first(), width);
            let row = point(args.get(1), height);
            let window = |at: u64, size: u64| {
                let span = cells.min(size);
                (at.saturating_sub(span / 2).min(size - span), span)
            };
            let (x, w) = window(column, width);
            let (y, h) = window(row, height);
            json!({ "x": x, "y": y, "w": w, "h": h, "cx": column - x, "cy": row - y })
        }
        _ => return Err(format!("Unknown colour picker function: {function}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Value {
        json!({ "accent": "#b4befe", "base": "#1e1e2e", "red": "#f38ba8" })
    }

    #[test]
    fn an_exact_palette_pixel_is_named_and_a_near_one_carries_its_distance() {
        let exact = entry([0xb4, 0xbe, 0xfe], Some(&palette()));
        assert_eq!(exact["token"], "accent");
        assert_eq!(exact["exact"], true);
        assert_eq!(exact["note"], "accent");
        assert_eq!(exact["hex"], "#b4befe");
        assert_eq!(exact["rgb"], "rgb(180, 190, 254)");
        let near = entry([0xb5, 0xbf, 0xfd], Some(&palette()));
        assert_eq!(near["exact"], false);
        assert_eq!(near["token"], "accent");
        assert!(near["note"].as_str().unwrap().starts_with("nearest accent · ΔE"));
        // Nothing in this palette is close to a saturated green.
        let far = entry([0x00, 0xff, 0x00], Some(&palette()));
        assert_eq!(far["token"], "");
        assert_eq!(far["note"], "");
        assert_eq!(far["exact"], false);
    }

    #[test]
    fn translucent_and_malformed_palette_entries_never_match() {
        let palette = json!({
            "wash": "#80b4befe", "solid": "#ffb4befe", "short": "#abc",
            "broken": "not a colour", "empty": "", "number": 7,
        });
        let entry = entry([0xb4, 0xbe, 0xfe], Some(&palette));
        assert_eq!(entry["token"], "solid");
        assert_eq!(entry["exact"], true);
        assert_eq!(super::parse("#abc"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(super::parse("#80b4befe"), None);
        assert_eq!(super::parse("#gg0000"), None);
        assert_eq!(super::parse("b4befe"), None);
    }

    #[test]
    fn formats_follow_what_the_colour_actually_offers() {
        let exact = entry([0xb4, 0xbe, 0xfe], Some(&palette()));
        let plain = entry([0x12, 0x34, 0x56], Some(&palette()));
        assert_eq!(call("cycle", &[exact.clone(), json!("hex")]).unwrap(), "rgb");
        assert_eq!(call("cycle", &[exact.clone(), json!("rgb")]).unwrap(), "token");
        assert_eq!(call("cycle", &[exact.clone(), json!("token")]).unwrap(), "hex");
        assert_eq!(call("cycle", &[plain.clone(), json!("rgb")]).unwrap(), "hex");
        assert_eq!(call("cycle", &[plain.clone(), json!("token")]).unwrap(), "hex");
        let copied = call("payload", &[exact.clone(), json!("token")]).unwrap();
        assert_eq!(copied["text"], "accent");
        assert_eq!(copied["format"], "token");
        // A recalled colour that is not a token still copies, as hex.
        let fell_back = call("payload", &[plain, json!("token")]).unwrap();
        assert_eq!(fell_back["text"], "#123456");
        assert_eq!(fell_back["format"], "hex");
        assert!(call("payload", &[json!({}), json!("hex")]).is_err());
    }

    fn hexes(history: &Value) -> Vec<String> {
        history
            .as_array()
            .unwrap()
            .iter()
            .map(|row| text(row.get("hex")))
            .collect()
    }

    #[test]
    fn history_bounds_itself_and_promotes_a_repeated_colour() {
        let entries = (0..12)
            .map(|index| json!({ "hex": format!("#0000{index:02x}") }))
            .collect::<Vec<_>>();
        let mut history = json!([]);
        for entry in &entries {
            history = call("history", &[history, entry.clone(), json!(9)]).unwrap();
        }
        assert_eq!(hexes(&history).len(), 9);
        assert_eq!(hexes(&history)[0], "#00000b");
        assert_eq!(hexes(&history)[8], "#000003");
        let promoted = call("history", &[history, entries[5].clone(), json!(9)]).unwrap();
        let rows = hexes(&promoted);
        assert_eq!(rows.len(), 9);
        assert_eq!(rows[0], "#000005");
        assert_eq!(rows.iter().filter(|hex| *hex == "#000005").count(), 1);
        assert!(call("history", &[json!([]), json!({}), json!(9)]).is_err());
    }

    #[test]
    fn the_lens_keeps_its_size_against_every_edge() {
        let cell = |lens: &Value, key: &str| lens[key].as_u64().unwrap();
        for (x, y) in [(0.0, 0.0), (0.5, 0.5), (1.0, 1.0), (-1.0, 2.0)] {
            let lens =
                call("loupe", &[json!(x), json!(y), json!(1920), json!(1080), json!(9)]).unwrap();
            assert_eq!(cell(&lens, "w"), 9);
            assert_eq!(cell(&lens, "h"), 9);
            assert!(cell(&lens, "x") + 9 <= 1920);
            assert!(cell(&lens, "y") + 9 <= 1080);
            assert!(cell(&lens, "cx") < 9);
            assert!(cell(&lens, "cy") < 9);
        }
        // A frame smaller than the lens gives it everything there is.
        let small = call("loupe", &[json!(0.5), json!(0.5), json!(4), json!(3), json!(9)]).unwrap();
        assert_eq!(cell(&small, "x"), 0);
        assert_eq!(cell(&small, "y"), 0);
        assert_eq!(cell(&small, "w"), 4);
        assert_eq!(cell(&small, "h"), 3);
        assert_eq!(cell(&small, "cx"), 2);
        assert_eq!(cell(&small, "cy"), 1);
        assert!(call("loupe", &[json!(0.5), json!(0.5), json!(0), json!(0), json!(9)]).is_err());
    }
}
