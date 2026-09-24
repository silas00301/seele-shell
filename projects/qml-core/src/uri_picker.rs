use crate::value::{array, finite, string, text, truthy};
use serde_json::{Value, json};
const MAX_RENDER_REGIONS: usize = 1024;
const GRID_SIDE: usize = 32;
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

struct SpatialIndex {
    width: f64,
    height: f64,
    boxes: Vec<Box>,
    bins: Vec<Vec<usize>>,
}
impl SpatialIndex {
    fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            boxes: Vec::new(),
            bins: vec![Vec::new(); GRID_SIDE * GRID_SIDE],
        }
    }
    fn cells(&self, b: Box) -> Option<(usize, usize, usize, usize)> {
        if self.width <= 0.0 || self.height <= 0.0 || b.w <= 0.0 || b.h <= 0.0 {
            return None;
        }
        let left = b.x.clamp(0.0, self.width);
        let top = b.y.clamp(0.0, self.height);
        let right = (b.x + b.w).clamp(0.0, self.width);
        let bottom = (b.y + b.h).clamp(0.0, self.height);
        if right <= left || bottom <= top {
            return None;
        }
        let start = |value: f64, extent: f64| {
            ((value / extent * GRID_SIDE as f64).floor() as usize).min(GRID_SIDE - 1)
        };
        let end = |value: f64, extent: f64| {
            ((value / extent * GRID_SIDE as f64).ceil() as usize)
                .saturating_sub(1)
                .min(GRID_SIDE - 1)
        };
        Some((
            start(left, self.width),
            end(right, self.width),
            start(top, self.height),
            end(bottom, self.height),
        ))
    }
    fn insert(&mut self, b: Box) {
        let index = self.boxes.len();
        self.boxes.push(b);
        if let Some((x0, x1, y0, y1)) = self.cells(b) {
            for y in y0..=y1 {
                for x in x0..=x1 {
                    self.bins[y * GRID_SIDE + x].push(index);
                }
            }
        }
    }
    fn overlap(&self, candidate: Box) -> f64 {
        let Some((x0, x1, y0, y1)) = self.cells(candidate) else {
            return 0.0;
        };
        let mut candidates = Vec::new();
        for y in y0..=y1 {
            for x in x0..=x1 {
                candidates.extend_from_slice(&self.bins[y * GRID_SIDE + x]);
            }
        }
        candidates.sort_unstable();
        candidates.dedup();
        candidates
            .into_iter()
            .map(|index| candidate.overlap(self.boxes[index]))
            .sum()
    }
}

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
            if local.len() > MAX_RENDER_REGIONS {
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
            // Anchor each badge to its link's first line, but avoid every
            // continuation highlight when scoring candidate placements.
            let mut occupied = SpatialIndex::new(width, height);
            let mut region_count = 0usize;
            for (link, first) in local.iter().zip(&boxes) {
                let regions = array(link.get("regions"));
                let count = regions.len().max(1);
                if count > 9 || region_count.saturating_add(count) > MAX_RENDER_REGIONS {
                    return Err("Too many code overlay regions".into());
                }
                region_count += count;
                if regions.is_empty() {
                    occupied.insert(*first);
                } else {
                    for region in regions {
                        occupied.insert(Box {
                            x: finite(region.get("x0"), 0.0) * width,
                            y: finite(region.get("y0"), 0.0) * height,
                            w: finite(region.get("w"), 0.0) * width,
                            h: finite(region.get("h"), 0.0) * height,
                        });
                    }
                }
            }
            let mut placed = SpatialIndex::new(width, height);
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
                        + occupied.overlap(candidate)
                        + placed.overlap(candidate) * 10.0;
                    if cost < score {
                        best = candidate;
                        score = cost;
                    }
                }
                placed.insert(best);
                result.insert(string(link.get("number")), best.json());
            }
            Value::Object(result)
        }
        _ => return Err(format!("Unknown URI picker function: {function}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_index_only_scores_intersecting_bins() {
        let mut index = SpatialIndex::new(1000.0, 1000.0);
        index.insert(Box {
            x: 10.0,
            y: 10.0,
            w: 30.0,
            h: 30.0,
        });
        index.insert(Box {
            x: 900.0,
            y: 900.0,
            w: 30.0,
            h: 30.0,
        });
        assert_eq!(
            index.overlap(Box {
                x: 20.0,
                y: 20.0,
                w: 10.0,
                h: 10.0
            }),
            100.0
        );
        assert_eq!(
            index.overlap(Box {
                x: 500.0,
                y: 500.0,
                w: 10.0,
                h: 10.0
            }),
            0.0
        );
    }

    #[test]
    fn layout_rejects_more_regions_than_qml_may_render() {
        let links = (0..=MAX_RENDER_REGIONS)
            .map(|number| {
                json!({
                    "number": number + 1,
                    "output": "DP-1",
                    "x0": 0.1,
                    "y0": 0.1,
                    "w": 0.01,
                    "h": 0.01,
                    "regions": [{"x0":0.1,"y0":0.1,"w":0.01,"h":0.01}]
                })
            })
            .collect::<Vec<_>>();
        assert!(
            call(
                "layout",
                &[
                    json!(links),
                    json!("DP-1"),
                    json!(1920),
                    json!(1080),
                    json!(32),
                    json!(24),
                    json!(4)
                ]
            )
            .is_err()
        );
    }
}
