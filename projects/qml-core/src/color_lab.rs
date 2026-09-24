//! Opaque sRGB workbench. Reject alpha rather than silently comparing a colour
//! composited over an unknown backdrop. WCAG comparisons use unrounded ratios.
use serde_json::{Value, json};

type Rgb = [u8; 3];
const INPUT_LIMIT: usize = 96;

fn number(input: &str, max: f64) -> Option<f64> {
    let value: f64 = input.trim().parse().ok()?;
    (value.is_finite() && (0.0..=max).contains(&value)).then_some(value)
}
fn parse(input: &str) -> Option<Rgb> {
    if input.len() > INPUT_LIMIT || !input.is_ascii() {
        return None;
    }
    let input = input.trim();
    if let Some(hex) = input.strip_prefix('#') {
        if !hex.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        return match hex.len() {
            3 => Some([
                u8::from_str_radix(&hex[0..1], 16).ok()? * 17,
                u8::from_str_radix(&hex[1..2], 16).ok()? * 17,
                u8::from_str_radix(&hex[2..3], 16).ok()? * 17,
            ]),
            6 => Some([
                u8::from_str_radix(&hex[0..2], 16).ok()?,
                u8::from_str_radix(&hex[2..4], 16).ok()?,
                u8::from_str_radix(&hex[4..6], 16).ok()?,
            ]),
            _ => None,
        };
    }
    let lower = input.to_ascii_lowercase();
    let (mode, body) = lower.split_once('(')?;
    let body = body.strip_suffix(')')?;
    let parts: Vec<_> = if body.contains(',') {
        body.split(',').map(str::trim).collect()
    } else {
        body.split_whitespace().collect()
    };
    if parts.len() != 3 {
        return None;
    }
    match mode {
        "rgb" => {
            let percent = parts[0].ends_with('%');
            let mut channels = [0; 3];
            for (index, part) in parts.iter().enumerate() {
                if part.ends_with('%') != percent {
                    return None;
                }
                channels[index] = if percent {
                    (number(part.strip_suffix('%')?, 100.0)? / 100.0 * 255.0).round() as u8
                } else {
                    number(part, 255.0)?.round() as u8
                };
            }
            Some(channels)
        }
        "hsl" => Some(from_hsl(
            number(parts[0].strip_suffix("deg").unwrap_or(parts[0]), 360.0)?,
            number(parts[1].strip_suffix('%')?, 100.0)? / 100.0,
            number(parts[2].strip_suffix('%')?, 100.0)? / 100.0,
        )),
        _ => None,
    }
}
fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h = (h % 360.0) / 60.0;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let rgb = match h as u8 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    rgb.map(|v| ((v + l - c / 2.0) * 255.0).round().clamp(0.0, 255.0) as u8)
}
fn hsl(rgb: Rgb) -> [f64; 3] {
    let [r, g, b] = rgb.map(|v| f64::from(v) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    if d == 0.0 {
        return [0.0, 0.0, l];
    }
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    [h, d / (1.0 - (2.0 * l - 1.0).abs()), l]
}
fn hex([r, g, b]: Rgb) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}
fn describe(rgb: Rgb) -> Value {
    let [r, g, b] = rgb;
    let [h, s, l] = hsl(rgb);
    json!({"hex":hex(rgb),"rgb":format!("rgb({r}, {g}, {b})"),"hsl":format!("hsl({h:.2}, {:.2}%, {:.2}%)",s*100.0,l*100.0)})
}
fn luminance(rgb: Rgb) -> f64 {
    let [r, g, b] = rgb.map(|v| {
        let c = f64::from(v) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}
fn ratio(fg: Rgb, bg: Rgb) -> f64 {
    let a = luminance(fg);
    let b = luminance(bg);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}
fn input(args: &[Value], index: usize) -> Option<Rgb> {
    parse(args.get(index)?.as_str()?)
}
fn view(args: &[Value]) -> Value {
    let fg = input(args, 0);
    let bg = input(args, 1);
    let (Some(fg), Some(bg)) = (fg, bg) else {
        return json!({"valid":false,"foregroundValid":fg.is_some(),"backgroundValid":bg.is_some(),"error":"Use opaque #RGB, #RRGGBB, rgb() or hsl(). Alpha is not supported."});
    };
    let ratio = ratio(fg, bg);
    let grades = [("Normal AA",4.5),("Normal AAA",7.0),("Large AA",3.0),("Large AAA",4.5),("UI / graphics",3.0)].map(|(label,threshold)|json!({"label":label,"threshold":format!("{threshold}:1"),"pass":ratio>=threshold}));
    json!({"valid":true,"foregroundValid":true,"backgroundValid":true,"foreground":describe(fg),"background":describe(bg),"ratio":ratio,"ratioText":format!("{ratio:.2}:1"),"grades":grades,"error":"","css":format!("color: {};\nbackground-color: {};",hex(fg),hex(bg))})
}
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    match function {
        "view" => Ok(view(args)),
        "palette" => {
            let Some(rgb) = input(args, 0) else {
                return Ok(json!([]));
            };
            let Some(other) = input(args, 1) else {
                return Ok(json!([]));
            };
            let [h, s, _] = hsl(rgb);
            Ok(Value::Array((0..9).map(|i| { let rgb=from_hsl(h,s,0.1+f64::from(i)*0.1); let contrast=ratio(rgb,other); json!({"hex":hex(rgb),"ratioText":format!("{contrast:.2}:1"),"pass":contrast>=4.5,"lightness":(i+1)*10}) }).collect()))
        }
        "copy" => {
            let view = view(args);
            if view["valid"] != true {
                return Err("Enter two valid opaque colours before copying".into());
            }
            let format = args.get(2).and_then(Value::as_str).unwrap_or("");
            if format == "css" {
                return Ok(view["css"].clone());
            }
            if !["hex", "rgb", "hsl"].contains(&format) {
                return Err("Unknown colour format".into());
            }
            let side = args.get(3).and_then(Value::as_str).unwrap_or("");
            if !["foreground", "background"].contains(&side) {
                return Err("Unknown colour target".into());
            }
            Ok(view[side][format].clone())
        }
        _ => Err("unknown color_lab function".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_ratios_and_unrounded_grades() {
        assert_eq!(ratio([0, 0, 0], [255, 255, 255]), 21.0);
        assert_eq!(ratio([30, 30, 46], [30, 30, 46]), 1.0);
        assert!((ratio([255, 0, 0], [255, 255, 255]) - 3.9984767707539985).abs() < 1e-12);
        // The displayed 4.50 must not turn a 4.498... failure into a pass.
        let boundary = view(&[json!("#070707"), json!("#777777")]);
        assert_eq!(boundary["ratioText"], "4.50:1");
        assert_eq!(boundary["grades"][0]["pass"], false);
        assert_eq!(boundary["grades"][3]["pass"], false);
        let grey = view(&[json!("#777"), json!("#fff")]);
        assert_eq!(grey["ratioText"], "4.48:1");
        assert_eq!(grey["grades"][0]["pass"], false);
        assert_eq!(grey["grades"][2]["pass"], true);
        assert_eq!(
            ratio([14, 125, 70], [124, 9, 75]),
            ratio([124, 9, 75], [14, 125, 70])
        );
    }
    #[test]
    fn conversions_and_round_trip() {
        assert_eq!(parse("#AbC"), Some([170, 187, 204]));
        assert_eq!(parse("rgb(100% 0% 0%)"), Some([255, 0, 0]));
        assert_eq!(parse("rgb(50% 10% 30%)"), Some([128, 26, 77]));
        assert_eq!(parse("hsl(360, 100%, 50%)"), Some([255, 0, 0]));
        for r in (0..=255).step_by(17) {
            for g in (0..=255).step_by(17) {
                for b in (0..=255).step_by(17) {
                    let rgb = [r, g, b];
                    let described = describe(rgb);
                    for mode in ["hex", "rgb", "hsl"] {
                        assert_eq!(parse(described[mode].as_str().unwrap()), Some(rgb));
                    }
                }
            }
        }
    }
    #[test]
    fn bounded_strict_and_consistent_alpha() {
        for value in [
            "#abcd",
            "#123456ff",
            "#ff123456",
            "rgba(1,2,3,1)",
            "rgb(1 2 3 / 1)",
            "hsla(0,0%,0%,1)",
            "red",
            "#💜ff",
            "rgb(NaN,0,0)",
            "rgb(256,0,0)",
            "rgb(-1,0,0)",
            "rgb(10%,0,0)",
            "hsl(361,50%,50%)",
            "hsl(0,101%,50%)",
            "",
        ] {
            assert_eq!(parse(value), None, "{value}");
        }
        assert!(parse(&" ".repeat(INPUT_LIMIT + 1)).is_none());
        assert_eq!(
            view(&[json!(null), json!("#fff")])["foregroundValid"],
            false
        );
        assert!(
            call(
                "copy",
                &[
                    json!("#ff000080"),
                    json!("#fff"),
                    json!("hex"),
                    json!("foreground")
                ]
            )
            .is_err()
        );
    }
    #[test]
    fn palette_and_copy_contract() {
        let args = [json!("#f00"), json!("#fff")];
        let ramp = call("palette", &args).unwrap();
        assert_eq!(ramp.as_array().unwrap().len(), 9);
        for pair in ramp.as_array().unwrap().windows(2) {
            assert!(
                luminance(parse(pair[0]["hex"].as_str().unwrap()).unwrap())
                    < luminance(parse(pair[1]["hex"].as_str().unwrap()).unwrap())
            );
        }
        assert_eq!(
            call("copy", &[json!("#f00"), json!("#fff"), json!("css")]).unwrap(),
            "color: #ff0000;\nbackground-color: #ffffff;"
        );
        assert_eq!(
            call(
                "copy",
                &[
                    json!("#f00"),
                    json!("#fff"),
                    json!("rgb"),
                    json!("background")
                ]
            )
            .unwrap(),
            "rgb(255, 255, 255)"
        );
        assert!(call("copy", &[json!("#f00"), json!("#fff"), json!("script")]).is_err());
        assert!(
            call(
                "copy",
                &[json!("#f00"), json!("#fff"), json!("hex"), json!("unknown")]
            )
            .is_err()
        );
    }
}
