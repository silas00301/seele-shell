//! Material 3 Expressive: light and dark colour schemes grown from one seed
//! colour, Material's type, shape, elevation, state and motion tokens, and the
//! theme document that applies them to the shell's contract.
//!
//! `seele-theme material` prints that document or the tokens themselves, and
//! registration holds a theme folder that records its seed to exactly what
//! the seed grows. See `themes/README.md`.
pub mod hct;
pub mod scheme;
pub mod tokens;

use crate::Result;
use serde_json::Value;

/// The seed a document names, parsed: `#rrggbb`.
pub fn seed(text: &str) -> Result<hct::Argb> {
    hct::parse(text).ok_or_else(|| format!("seed: {text:?} is not a #rrggbb colour").into())
}

/// A contrast level from -1 to 1; Material's default is 0.
pub fn contrast(value: f64) -> Result<f64> {
    if (-1.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(format!("contrast: {value} is not between -1 and 1").into())
    }
}

/// The document a folder's `material` source grows into, for the identity the
/// folder gives it, or why its source cannot grow one.
pub fn regrown(document: &Value) -> std::result::Result<Value, String> {
    let source = &document["material"];
    let seed = source["seed"]
        .as_str()
        .and_then(hct::parse)
        .ok_or("material.seed: not a #rrggbb colour")?;
    let level = source["contrast"]
        .as_f64()
        .filter(|level| (-1.0..=1.0).contains(level))
        .ok_or("material.contrast: not a number from -1 to 1")?;
    if source.as_object().is_some_and(|object| object.len() != 2) {
        return Err("material: names only its seed and contrast".to_owned());
    }
    let (id, name) = (
        document["id"].as_str().unwrap_or_default(),
        document["name"].as_str().unwrap_or_default(),
    );
    let dark = match document["mode"].as_str() {
        Some("dark") => true,
        Some("light") => false,
        _ => return Err("material: needs the theme's mode".to_owned()),
    };
    Ok(tokens::document(seed, dark, level, id, name))
}

/// Every way `document` differs from what its `material` source grows, by
/// slot, or none.
pub fn drift(document: &Value) -> Vec<String> {
    let grown = match regrown(document) {
        Ok(grown) => grown,
        Err(error) => return vec![error],
    };
    let (Some(grown), Some(found)) = (grown.as_object(), document.as_object()) else {
        return vec!["a theme document is a JSON object".to_owned()];
    };
    let mut keys: Vec<&String> = grown.keys().chain(found.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter()
        .filter(|key| !same(grown.get(*key), found.get(*key)))
        .map(|key| match (found.get(key), grown.get(key)) {
            (Some(found), Some(grown)) => format!("{key}: {found} here, {grown} from its seed"),
            (None, Some(grown)) => format!("{key}: missing, {grown} from its seed"),
            (Some(_), None) => format!("{key}: not something its seed sets"),
            (None, None) => unreachable!(),
        })
        .collect()
}

/// JSON equality that reads `0` and `0.0` as one number, and a real as what it
/// parses back to, since a document's numbers pass through text.
fn same(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (Some(Value::Number(a)), Some(Value::Number(b))) => {
            let (a, b) = (
                a.as_f64().unwrap_or(f64::NAN),
                b.as_f64().unwrap_or(f64::NAN),
            );
            (a - b).abs() <= 1e-12 * a.abs().max(1.0)
        }
        (Some(Value::Array(a)), Some(Value::Array(b))) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(Some(a), Some(b)))
        }
        (Some(Value::Object(a)), Some(Value::Object(b))) => {
            a.len() == b.len() && a.iter().all(|(key, value)| same(Some(value), b.get(key)))
        }
        (a, b) => a == b,
    }
}
