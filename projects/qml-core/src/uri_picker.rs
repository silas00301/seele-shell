use crate::value::{array, finite, string, text, truthy};
use serde_json::{Value, json};
#[derive(Clone, Copy)]
struct Box {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
impl Box {
    fn from(value: &Value) -> Self {
        Self {
            x: finite(value.get("x"), 0.0),
            y: finite(value.get("y"), 0.0),
            w: finite(value.get("w"), 0.0),
            h: finite(value.get("h"), 0.0),
        }
    }
    fn json(self) -> Value {
        json!({"x":self.x,"y":self.y,"w":self.w,"h":self.h})
    }
    fn overlap(self, b: Self) -> f64 {
        (self.x + self.w)
            .min(b.x + b.w)
            .sub(self.x.max(b.x))
            .max(0.0)
            * (self.y + self.h)
                .min(b.y + b.h)
                .sub(self.y.max(b.y))
                .max(0.0)
    }
}
use std::ops::Sub;
pub fn call(function: &str, args: &[Value]) -> Result<Value, String> {
    let null = Value::Null;
    Ok(match function {
        "selection" => {
            let digits = text(args.get(1));
            if digits.is_empty() {
                return Ok(Value::Null);
            }
            let mut exact = None;
            let mut matches = 0;
            for (i, link) in array(args.first()).iter().enumerate() {
                let number = string(link.get("number"));
                if number.starts_with(&digits) {
                    matches += 1;
                }
                if number == digits {
                    exact = Some(i);
                }
            }
            if truthy(args.get(3)) || truthy(args.get(2)) && matches == 1 {
                json!(exact)
            } else {
                Value::Null
            }
        }
        "overlap" => json!(
            Box::from(args.first().unwrap_or(&null))
                .overlap(Box::from(args.get(1).unwrap_or(&null)))
        ),
        "caption" => {
            let b = Box::from(args.first().unwrap_or(&null));
            let width = finite(args.get(1), 0.0).max(0.0);
            let height = finite(args.get(2), 0.0).max(0.0);
            let text_width = finite(args.get(3), 0.0).max(0.0);
            let text_height = finite(args.get(4), 0.0).max(0.0);
            let gap = finite(args.get(5), 0.0);
            let w = width.min(text_width);
            let below = (height - b.y - b.h - gap).max(0.0);
            let above = (b.y - gap).max(0.0);
            let use_below = text_height <= below || text_height > above && below >= above;
            let h = text_height.min(if use_below { below } else { above });
            Box {
                x: (b.x + (b.w - w) / 2.0).min(width - w).max(0.0),
                y: if use_below {
                    b.y + b.h + gap
                } else {
                    b.y - gap - h
                },
                w,
                h,
            }
            .json()
        }
        "layout" => {
            let output = text(args.get(1));
            let width = finite(args.get(2), 0.0).max(0.0);
            let height = finite(args.get(3), 0.0).max(0.0);
            let bw = finite(args.get(4), 0.0).max(0.0);
            let bh = finite(args.get(5), 0.0).max(0.0);
            let gap = finite(args.get(6), 0.0);
            let local: Vec<_> = array(args.first())
                .iter()
                .filter(|link| link["output"].as_str() == Some(&output))
                .collect();
            if local.len() > 4096 {
                return Err("Too many code overlays".into());
            }
            let boxes: Vec<_> = local
                .iter()
                .map(|link| Box {
                    x: finite(link.get("x0"), 0.0) * width,
                    y: finite(link.get("y0"), 0.0) * height,
                    w: finite(link.get("w"), 0.0) * width,
                    h: finite(link.get("h"), 0.0) * height,
                })
                .collect();
            let mut placed = Vec::<Box>::new();
            let mut result = serde_json::Map::new();
            for (link, b) in local.iter().zip(&boxes) {
                let candidates = [
                    (b.x - bw - gap, b.y + (b.h - bh) / 2.0),
                    (b.x + b.w + gap, b.y + (b.h - bh) / 2.0),
                    (b.x, b.y - bh - gap),
                    (b.x, b.y + b.h + gap),
                ];
                let mut best = Box {
                    x: 0.0,
                    y: 0.0,
                    w: bw,
                    h: bh,
                };
                let mut score = f64::INFINITY;
                for (x, y) in candidates {
                    let candidate = Box {
                        x: x.min(width - bw).max(0.0),
                        y: y.min(height - bh).max(0.0),
                        w: bw,
                        h: bh,
                    };
                    let cost = (x - candidate.x).abs()
                        + (y - candidate.y).abs()
                        + boxes.iter().map(|b| candidate.overlap(*b)).sum::<f64>()
                        + placed
                            .iter()
                            .map(|b| candidate.overlap(*b) * 10.0)
                            .sum::<f64>();
                    if cost < score {
                        best = candidate;
                        score = cost;
                    }
                }
                placed.push(best);
                result.insert(string(link.get("number")), best.json());
            }
            Value::Object(result)
        }
        _ => return Err(format!("Unknown URI picker function: {function}")),
    })
}
