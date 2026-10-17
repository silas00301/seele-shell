//! Material's springs as timed curves.
//!
//! Material 3 Expressive moves components on springs: a damping ratio and a
//! stiffness, for a unit mass, released from rest one unit away from where it
//! comes to rest. Qt animates on a duration and an easing curve, and the theme
//! contract carries a curve as a Bézier spline of at most eight segments, so a
//! spring is sampled into both: the time it takes to come within a thousandth
//! of its travel and stay there, and its motion over that time.
//!
//! Each segment is the cubic Hermite interpolant of the spring between two
//! instants, written as a Bézier whose control points sit a third of the way
//! along the segment in time. Time therefore advances linearly within a
//! segment, Qt's solve for it is exact, and the curve passes through the
//! spring's own position and velocity at every joint. Segments are shorter
//! early, where the spring moves fastest; the spacing is the one, from a fixed
//! ladder, that keeps the whole curve closest to the spring.

/// The spring's distance from rest, as a share of its travel, within which it
/// counts as settled.
const SETTLED: f64 = 0.001;
/// The segments a curve is drawn with: the contract's limit.
const SEGMENTS: usize = 8;
/// The spacing exponents tried, from even (1) to strongly front-loaded (3).
const SPACINGS: std::ops::RangeInclusive<u32> = 20..=60;
/// Control points are written to this many decimal places.
const PLACES: f64 = 1e4;

/// A spring of `damping` ratio and `stiffness`, from rest at 0 toward rest at 1.
#[derive(Clone, Copy, Debug)]
pub struct Spring {
    pub damping: f64,
    pub stiffness: f64,
}

impl Spring {
    fn frequency(self) -> f64 {
        self.stiffness.sqrt()
    }

    /// Where the spring is, `t` seconds after its release.
    pub fn position(self, t: f64) -> f64 {
        let (zeta, omega) = (self.damping, self.frequency());
        if zeta < 1.0 {
            let damped = omega * (1.0 - zeta * zeta).sqrt();
            let decay = (-zeta * omega * t).exp();
            1.0 - decay * ((damped * t).cos() + zeta * omega / damped * (damped * t).sin())
        } else {
            1.0 - (1.0 + omega * t) * (-omega * t).exp()
        }
    }

    /// How fast it moves then, in travel per second.
    pub fn velocity(self, t: f64) -> f64 {
        let (zeta, omega) = (self.damping, self.frequency());
        if zeta < 1.0 {
            let damped = omega * (1.0 - zeta * zeta).sqrt();
            (-zeta * omega * t).exp() * omega * omega / damped * (damped * t).sin()
        } else {
            omega * omega * t * (-omega * t).exp()
        }
    }

    /// When the spring is settled: the bound on its distance from rest has
    /// fallen to `SETTLED`. Under critical damping the bound is the distance
    /// itself; below it, the envelope its oscillation decays inside.
    pub fn settle(self) -> f64 {
        let (zeta, omega) = (self.damping, self.frequency());
        if zeta < 1.0 {
            (1.0 / (SETTLED * (1.0 - zeta * zeta).sqrt())).ln() / (zeta * omega)
        } else {
            let (mut low, mut high) = (0.0, 100.0 / omega);
            for _ in 0..200 {
                let middle = (low + high) / 2.0;
                if (1.0 + omega * middle) * (-omega * middle).exp() > SETTLED {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            high
        }
    }

    /// The spring as the contract's motion: whole milliseconds, and the curve
    /// it follows over them.
    pub fn timed(self) -> (u32, Vec<f64>) {
        let duration = (self.settle() * 1000.0).ceil();
        let span = duration / 1000.0;
        let curve = SPACINGS
            .map(|step| self.curve(span, f64::from(step) / 20.0))
            .min_by(|a, b| self.error(span, a).total_cmp(&self.error(span, b)))
            .expect("a spacing");
        (duration as u32, curve)
    }

    /// The spline over `span` seconds with joints at `(i / n)^spacing`.
    fn curve(self, span: f64, spacing: f64) -> Vec<f64> {
        let joint = |index: usize| (index as f64 / SEGMENTS as f64).powf(spacing);
        let round = |value: f64| (value * PLACES).round() / PLACES;
        let mut points = Vec::with_capacity(SEGMENTS * 6);
        for index in 0..SEGMENTS {
            let (from, to) = (joint(index), joint(index + 1));
            let third = (to - from) / 3.0;
            let start = self.position(from * span);
            // The last joint is rest itself, as Qt's curve must end there.
            let (end, slope) = if index + 1 == SEGMENTS {
                (1.0, 0.0)
            } else {
                (self.position(to * span), self.velocity(to * span) * span)
            };
            points.extend([
                from + third,
                start + self.velocity(from * span) * span * third,
                to - third,
                end - slope * third,
                to,
                end,
            ]);
        }
        points.into_iter().map(round).collect()
    }

    /// The largest distance between `curve` and the spring over `span`.
    pub fn error(self, span: f64, curve: &[f64]) -> f64 {
        (0..=2000)
            .map(|step| {
                let x = f64::from(step) / 2000.0;
                (evaluate(curve, x) - self.position(x * span)).abs()
            })
            .fold(0.0, f64::max)
    }
}

/// A spline of segments whose control points lie a third of the way along in
/// time, as `curve` writes them, at time `x`.
pub fn evaluate(curve: &[f64], x: f64) -> f64 {
    let mut start = (0.0, 0.0);
    for segment in curve.chunks(6) {
        let [_, y1, _, y2, x3, y3] = [
            segment[0], segment[1], segment[2], segment[3], segment[4], segment[5],
        ];
        if x <= x3 {
            let along = if x3 > start.0 {
                (x - start.0) / (x3 - start.0)
            } else {
                1.0
            };
            let rest = 1.0 - along;
            return rest.powi(3) * start.1
                + 3.0 * rest * rest * along * y1
                + 3.0 * rest * along * along * y2
                + along.powi(3) * y3;
        }
        start = (x3, y3);
    }
    start.1
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST_SPATIAL: Spring = Spring {
        damping: 0.6,
        stiffness: 800.0,
    };
    const FAST_EFFECTS: Spring = Spring {
        damping: 1.0,
        stiffness: 3800.0,
    };

    #[test]
    fn a_spring_settles_within_a_thousandth_of_its_travel() {
        for spring in [FAST_SPATIAL, FAST_EFFECTS] {
            let settle = spring.settle();
            for step in 0..1000 {
                let t = settle * (1.0 + f64::from(step) / 100.0);
                assert!(
                    (spring.position(t) - 1.0).abs() <= SETTLED + 1e-12,
                    "{spring:?} at {t}"
                );
            }
        }
        // Material's fast spatial spring overshoots by a tenth and settles
        // in 421 ms; the fast effects spring never overshoots, in 150 ms.
        assert_eq!(FAST_SPATIAL.timed().0, 421);
        assert_eq!(FAST_EFFECTS.timed().0, 150);
        let peak = (0..1000)
            .map(|step| FAST_SPATIAL.position(f64::from(step) / 1000.0))
            .fold(0.0, f64::max);
        assert!((1.09..1.10).contains(&peak), "{peak}");
    }

    #[test]
    fn the_curve_is_a_valid_spline_close_to_the_spring() {
        for spring in [FAST_SPATIAL, FAST_EFFECTS] {
            let (duration, curve) = spring.timed();
            assert_eq!(curve.len(), SEGMENTS * 6);
            assert_eq!(&curve[curve.len() - 2..], &[1.0, 1.0]);
            // Time only moves forward, so Qt can solve each segment for it.
            let times: Vec<f64> = curve.chunks(2).map(|point| point[0]).collect();
            assert!(times.windows(2).all(|pair| pair[0] < pair[1]), "{times:?}");
            let error = spring.error(f64::from(duration) / 1000.0, &curve);
            assert!(error < 0.002, "{spring:?}: {error}");
        }
    }

    #[test]
    fn evaluate_follows_the_joints() {
        let (duration, curve) = FAST_SPATIAL.timed();
        let span = f64::from(duration) / 1000.0;
        for joint in curve.chunks(6).take(SEGMENTS - 1) {
            assert!(
                (evaluate(&curve, joint[4]) - FAST_SPATIAL.position(joint[4] * span)).abs() < 1e-3
            );
        }
        assert_eq!(evaluate(&curve, 0.0), 0.0);
        assert_eq!(evaluate(&curve, 1.0), 1.0);
    }
}
