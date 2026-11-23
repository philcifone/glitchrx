//! `curves`: a tone curve through a few points, as in Photoshop's Curves.
//!
//! The points are joined by a monotone cubic (Fritsch and Carlson, 1980):
//! a smooth curve that, unlike a plain spline, never overshoots between
//! points, so a rising set of points gives a rising curve and an S stays an
//! S without ringing at its ends. Left of the first point and right of the
//! last the curve is flat, so moving an end point in clips.

use super::adjust::{CHANNEL, apply_table, channels, table};
use super::{Effect, EffectInfo, Group, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "curves",
    group: Group::Adjust,
    version: 1,
    audacity: "none (an adjustment)",
    summary: "Reshape the tones with a curve through a few points.",
    details: "`points` lists `IN:OUT` pairs from 0 to 255, inputs rising, joined by `/`. \
              The default is a gentle S, which adds contrast; `0:0/255:255` changes \
              nothing, and `0:255/255:0` is a negative. The curve passes smoothly \
              through every point without overshooting. `channel` limits it to one colour.",
    params: &[
        CHANNEL,
        ParamSpec {
            name: "points",
            kind: Kind::Curve,
            default: "0:0/64:52/192:204/255:255",
            help: "IN:OUT pairs, 0 to 255, inputs rising, joined by /",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --curves points=0:20/128:150/255:235,channel=blue photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let pts: Vec<(f64, f64)> = p
        .curve("points")
        .iter()
        .map(|&(x, y)| (x as f64 / 255.0, y as f64 / 255.0))
        .collect();
    let m = tangents(&pts);
    Ok(Box::new(Curves {
        which: channels(p),
        table: table(|x| eval(&pts, &m, x / 255.0)),
    }))
}

/// Fritsch-Carlson tangents: the average of the neighbouring slopes, zero
/// at a peak or a flat stretch, then shortened where they would make the
/// cubic overshoot (the alpha^2 + beta^2 <= 9 condition).
pub(crate) fn tangents(p: &[(f64, f64)]) -> Vec<f64> {
    let n = p.len();
    let d: Vec<f64> = p
        .windows(2)
        .map(|w| (w[1].1 - w[0].1) / (w[1].0 - w[0].0))
        .collect();
    let mut m = vec![0.0; n];
    m[0] = d[0];
    m[n - 1] = d[n - 2];
    for k in 1..n - 1 {
        m[k] = if d[k - 1] * d[k] > 0.0 {
            (d[k - 1] + d[k]) / 2.0
        } else {
            0.0
        };
    }
    for k in 0..n - 1 {
        if d[k] == 0.0 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let (a, b) = (m[k] / d[k], m[k + 1] / d[k]);
        let s = a * a + b * b;
        if s > 9.0 {
            // sqrt is correctly rounded by IEEE 754, so it is as
            // deterministic as + - * / (D-25).
            let t = 3.0 / s.sqrt();
            m[k] = t * a * d[k];
            m[k + 1] = t * b * d[k];
        }
    }
    m
}

/// The curve at `x`: cubic Hermite between the points around it, flat
/// outside them.
pub(crate) fn eval(p: &[(f64, f64)], m: &[f64], x: f64) -> f64 {
    let n = p.len();
    if x <= p[0].0 {
        return p[0].1;
    }
    if x >= p[n - 1].0 {
        return p[n - 1].1;
    }
    let k = p.windows(2).position(|w| x < w[1].0).unwrap_or(n - 2);
    let ((x0, y0), (x1, y1)) = (p[k], p[k + 1]);
    let h = x1 - x0;
    let t = (x - x0) / h;
    let (t2, t3) = (t * t, t * t * t);
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    h00 * y0 + h10 * h * m[k] + h01 * y1 + h11 * h * m[k + 1]
}

struct Curves {
    which: [bool; 3],
    table: [u8; 256],
}

impl Effect for Curves {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        apply_table(w, &self.table, self.which)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn two_points_are_a_straight_line() {
        let e = make(&INFO, "points=0:255/255:0");
        let mut b = vec![0, 100, 255, 1, 2, 3];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![255, 155, 0, 254, 253, 252]);
        let e = make(&INFO, "points=0:0/255:255");
        let orig: Vec<u8> = (0..=254).collect();
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 5, Encoding::U8);
        assert_eq!(b, orig);
    }

    #[test]
    fn passes_through_its_points_and_never_overshoots() {
        let pts = [(0.0, 0.0), (0.2, 0.6), (0.25, 0.62), (1.0, 1.0)];
        let m = tangents(&pts);
        for &(x, y) in &pts {
            assert!((eval(&pts, &m, x) - y).abs() < 1e-12);
        }
        // Monotone data, monotone curve: no dip after the steep rise.
        let mut last = 0.0;
        for i in 0..=1000 {
            let v = eval(&pts, &m, i as f64 / 1000.0);
            assert!(v >= last - 1e-12 && v <= 1.0, "at {i}: {v} after {last}");
            last = v;
        }
    }

    #[test]
    fn flat_outside_the_end_points() {
        let e = make(&INFO, "points=50:20/200:230");
        let mut b = vec![0, 50, 210, 255, 30, 200];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![20, 20, 230, 230, 20, 230]);
    }
}
