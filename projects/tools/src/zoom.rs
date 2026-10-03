//! Screen zoom around the pointer through Hyprland's own `cursor:zoom_factor`.
//!
//! The compositor's option is the only record of the level. Every step reads
//! it back, so a reload, a relogin or a trackpad gesture that moved it can
//! never leave this tool stepping from a value it remembered instead. Levels
//! sit on a grid of quarter octaves: a scroll notch moves one quarter, a key
//! moves two, so both reach 2x, 4x and 8x exactly and stepping back down always
//! lands on 1, which Hyprland draws as the untouched frame.
use crate::control::hyprctl;
use crate::Result;
use serde_json::{json, Value};
use std::fs::OpenOptions;
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

const OPTION: &str = "cursor:zoom_factor";
/// Quarter octaves per scroll notch and per key press.
const FINE: i32 = 1;
const COARSE: i32 = 2;
/// 2^(12/4) = 8x. Hyprland itself accepts up to 10x.
const MAX_STEP: i32 = 12;
/// A level read back within this many quarter octaves of a grid point is on
/// it: Hyprland prints the factor with six decimals, far finer than this.
const SNAP: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    In,
    Out,
    Reset,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Request {
    action: Action,
    fine: bool,
}

pub fn request(arguments: &[String]) -> Result<Request> {
    let (action, rest) = arguments.split_first().ok_or("zoom action required")?;
    let action = match action.as_str() {
        "in" => Action::In,
        "out" => Action::Out,
        "reset" => Action::Reset,
        _ => return Err("zoom action must be in, out or reset".into()),
    };
    let fine = match rest {
        [] => false,
        [flag] if flag == "--fine" && action != Action::Reset => true,
        _ => return Err("zoom takes only --fine after in or out".into()),
    };
    Ok(Request { action, fine })
}

fn quarters(factor: f64) -> f64 {
    factor.log2() * 4.0
}

fn factor(step: i32) -> f64 {
    2f64.powf(f64::from(step.clamp(0, MAX_STEP)) / 4.0)
}

/// The level the request moves to from `current`, or `None` when it would not
/// move: zooming in at the limit, out at 1x, or resetting an unzoomed screen.
/// A level off the grid, left by a trackpad gesture or set by hand, steps to
/// the next grid point in the requested direction rather than past it.
pub fn next(current: f64, request: Request) -> Option<f64> {
    let current = if current.is_finite() && current > 1.0 {
        current
    } else {
        1.0
    };
    let at = quarters(current);
    let size = if request.fine { FINE } else { COARSE };
    let target = match request.action {
        Action::Reset => 0,
        Action::In => (at + SNAP).floor() as i32 + size,
        Action::Out => (at - SNAP).ceil() as i32 - size,
    }
    .clamp(0, MAX_STEP);
    let moves = match request.action {
        Action::In => f64::from(target) > at + SNAP,
        Action::Out | Action::Reset => current > 1.0,
    };
    moves.then(|| factor(target))
}

/// Hyprland's `getoption -j` reply for a float option.
pub fn parse_reply(reply: &str) -> Result<f64> {
    let value: Value =
        serde_json::from_str(reply.trim()).map_err(|_| "Hyprland has no zoom factor")?;
    if value["option"].as_str() != Some(OPTION) {
        return Err("Hyprland answered for another option".into());
    }
    value["float"]
        .as_f64()
        .filter(|factor| factor.is_finite() && *factor >= 1.0)
        .ok_or_else(|| "Hyprland reported an invalid zoom factor".into())
}

/// The literal handed to Lua: exactly `1` for the unzoomed screen, otherwise
/// six decimals with the trailing zeros dropped.
pub fn literal(factor: f64) -> String {
    let text = format!("{factor:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_owned()
}

/// `nerv` runs a Lua configuration, where `hyprctl keyword` is refused; the
/// option is set through the same `hl.config` call the configuration uses.
pub fn assignment(factor: f64) -> String {
    format!(
        "hl.config({{ cursor = {{ zoom_factor = {} }} }})",
        literal(factor)
    )
}

/// What the OSD draws. The label carries one decimal, and none for a whole
/// factor; the ratio fills the meter by octaves, so each doubling is a third.
pub fn presentation(factor: f64) -> Value {
    let rounded = (factor * 10.0).round() / 10.0;
    let label = if rounded.fract() == 0.0 {
        format!("{rounded:.0}×")
    } else {
        format!("{rounded:.1}×")
    };
    let ratio = (quarters(factor) / f64::from(MAX_STEP)).clamp(0.0, 1.0);
    json!({ "factor": factor, "zoomed": factor > 1.0, "label": label, "ratio": ratio })
}

fn read() -> Result<f64> {
    parse_reply(&hyprctl(&["getoption", OPTION, "-j"])?)
}

fn write(factor: f64) -> Result {
    let reply = hyprctl(&["eval", &assignment(factor)])?;
    if reply.trim() != "ok" {
        return Err("Hyprland rejected the zoom factor".into());
    }
    Ok(())
}

/// Serializes read, step and write, so scroll notches that arrive faster than
/// one round trip each still move one step rather than reading the same level.
/// The lock holds no state; the compositor's option remains the only record.
fn lock() -> Result<std::fs::File> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or("screen zoom needs a user session")?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(PathBuf::from(runtime).join("seele-zoom.lock"))?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err("screen zoom lock unavailable".into());
    }
    Ok(file)
}

/// Applies one request and returns what the OSD should show.
pub fn apply(request: Request) -> Result<Value> {
    let _lock = lock()?;
    let current = read()?;
    let level = match next(current, request) {
        Some(level) => {
            write(level)?;
            level
        }
        None => current,
    };
    Ok(presentation(level))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(action: Action, fine: bool) -> Request {
        Request { action, fine }
    }

    /// A level as Hyprland prints it, to six decimals.
    fn reported(text: &str) -> f64 {
        text.parse().unwrap()
    }

    fn walk(start: f64, request: Request) -> Vec<String> {
        let mut levels = vec![];
        let mut level = start;
        while let Some(next) = next(level, request) {
            levels.push(literal(next));
            // Hyprland prints six decimals, so each step reads its own write
            // back through that rounding.
            level = literal(next).parse().unwrap();
        }
        levels
    }

    #[test]
    fn requests_parse_strictly() {
        let arguments = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            request(&arguments(&["in"])).unwrap(),
            step(Action::In, false)
        );
        assert_eq!(
            request(&arguments(&["out", "--fine"])).unwrap(),
            step(Action::Out, true)
        );
        assert_eq!(
            request(&arguments(&["reset"])).unwrap(),
            step(Action::Reset, false)
        );
        for bad in [
            &[][..],
            &["sideways"],
            &["in", "--coarse"],
            &["reset", "--fine"],
            &["in", "--fine", "--fine"],
        ] {
            assert!(request(&arguments(bad)).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn keys_step_by_half_octaves_and_clamp_at_eight() {
        assert_eq!(
            walk(1.0, step(Action::In, false)),
            ["1.414214", "2", "2.828427", "4", "5.656854", "8"]
        );
        assert_eq!(next(8.0, step(Action::In, false)), None);
        assert_eq!(
            walk(8.0, step(Action::Out, false)),
            ["5.656854", "4", "2.828427", "2", "1.414214", "1"]
        );
    }

    #[test]
    fn scroll_steps_are_finer_and_share_the_grid() {
        let levels = walk(1.0, step(Action::In, true));
        assert_eq!(levels.len(), 12);
        assert_eq!(levels[0], "1.189207");
        assert_eq!(levels[3], "2");
        assert_eq!(levels[11], "8");
        let down = walk(8.0, step(Action::Out, true));
        assert_eq!(down.len(), 12);
        assert_eq!(down.last().unwrap(), "1");
    }

    #[test]
    fn stepping_down_restores_exactly_one() {
        assert_eq!(next(1.189207, step(Action::Out, false)), Some(1.0));
        assert_eq!(
            literal(next(reported("1.414214"), step(Action::Out, true)).unwrap()),
            "1.189207"
        );
        assert_eq!(next(1.0, step(Action::Out, true)), None);
        assert_eq!(next(1.0, step(Action::Reset, false)), None);
        assert_eq!(next(5.656854, step(Action::Reset, false)), Some(1.0));
        assert_eq!(literal(1.0), "1");
        assert_eq!(
            assignment(1.0),
            "hl.config({ cursor = { zoom_factor = 1 } })"
        );
    }

    #[test]
    fn levels_off_the_grid_step_to_the_next_grid_point() {
        // Between 1.41x and 1.68x, a trackpad pinch left 1.5x.
        assert_eq!(
            literal(next(1.5, step(Action::In, true)).unwrap()),
            "1.681793"
        );
        assert_eq!(
            literal(next(1.5, step(Action::Out, true)).unwrap()),
            "1.414214"
        );
        assert_eq!(literal(next(1.5, step(Action::In, false)).unwrap()), "2");
        // Beyond the limit, zooming in holds and zooming out comes back to it.
        assert_eq!(next(10.0, step(Action::In, true)), None);
        assert_eq!(next(10.0, step(Action::Out, true)), Some(8.0));
        // Nonsense from the compositor is treated as the unzoomed screen.
        assert_eq!(next(f64::NAN, step(Action::Out, false)), None);
        assert_eq!(
            literal(next(0.5, step(Action::In, false)).unwrap()),
            "1.414214"
        );
    }

    #[test]
    fn hyprland_replies_parse_and_reject() {
        assert_eq!(
            parse_reply(r#"{"option": "cursor:zoom_factor", "float": 2.000000, "set": true }"#)
                .unwrap(),
            2.0
        );
        assert_eq!(
            parse_reply(
                "{\"option\": \"cursor:zoom_factor\", \"float\": 1.000000, \"set\": false }\n"
            )
            .unwrap(),
            1.0
        );
        for reply in [
            "no such option",
            "",
            r#"{"option": "cursor:zoom_rigid", "float": 2.0, "set": true }"#,
            r#"{"option": "cursor:zoom_factor", "int": 2, "set": true }"#,
            r#"{"option": "cursor:zoom_factor", "float": 0.5, "set": true }"#,
            r#"{"option": "cursor:zoom_factor", "float": "2", "set": true }"#,
        ] {
            assert!(parse_reply(reply).is_err(), "{reply}");
        }
    }

    #[test]
    fn the_osd_reads_one_decimal_and_fills_by_octaves() {
        let shown = presentation(1.0);
        assert_eq!(shown["label"], "1×");
        assert_eq!(shown["zoomed"], false);
        assert_eq!(shown["ratio"], 0.0);
        assert_eq!(presentation(reported("1.414214"))["label"], "1.4×");
        assert_eq!(presentation(2.0)["label"], "2×");
        assert_eq!(presentation(5.656854)["label"], "5.7×");
        assert_eq!(presentation(8.0)["ratio"], 1.0);
        assert!((presentation(2.0)["ratio"].as_f64().unwrap() - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(presentation(10.0)["ratio"], 1.0);
    }
}
