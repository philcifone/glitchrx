//! `balance`: colour balance for the shadows, midtones or highlights, as
//! in Photoshop's Color Balance.
//!
//! Each slider pushes one channel against its opposite (cyan against red,
//! magenta against green, yellow against blue), weighted by how far the
//! pixel's brightness lies in the chosen tones. The weights are the three
//! quadratic Bernstein polynomials of the brightness L, which sum to one
//! across the range: shadows (1 - L)^2, midtones 2L(1 - L), highlights
//! L^2 (midtones doubled here so all three peak at 1). Keeping luminosity
//! then shifts all three channels equally to restore the brightness, so
//! only the colour changes.

use super::adjust::{luma, per_pixel, to_byte};
use super::{Effect, EffectInfo, Group, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "balance",
    group: Group::Adjust,
    version: 1,
    audacity: "none (an adjustment)",
    summary: "Tint the shadows, midtones or highlights.",
    details: "Pick the `tones`, then push them towards red or cyan, green or magenta, \
              blue or yellow (-100 to 100; at 100 a full-weight pixel moves by about 100 \
              of 255). Add a step for each range you want to tint: warm highlights and \
              cool shadows are two steps. `luminosity=keep` restores each pixel's \
              brightness so only the colour changes.",
    params: &[
        ParamSpec {
            name: "tones",
            kind: Kind::Choice(&["shadows", "midtones", "highlights"]),
            default: "midtones",
            help: "which tones are tinted",
        },
        ParamSpec {
            name: "red",
            kind: Kind::Int {
                min: -100,
                max: 100,
            },
            default: "12",
            help: "towards red; below 0 towards cyan",
        },
        ParamSpec {
            name: "green",
            kind: Kind::Int {
                min: -100,
                max: 100,
            },
            default: "0",
            help: "towards green; below 0 towards magenta",
        },
        ParamSpec {
            name: "blue",
            kind: Kind::Int {
                min: -100,
                max: 100,
            },
            default: "-12",
            help: "towards blue; below 0 towards yellow",
        },
        ParamSpec {
            name: "luminosity",
            kind: Kind::Choice(&["keep", "change"]),
            default: "keep",
            help: "keep each pixel's brightness, so only the colour changes",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --balance tones=shadows,blue=30 --balance tones=highlights,red=20,blue=-20 photo.jpg",
    build,
};

/// The most a full-weight pixel moves at 100, as a share of the range.
const REACH: f64 = 0.4;

#[derive(Clone, Copy)]
enum Tones {
    Shadows,
    Midtones,
    Highlights,
}

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let tones = match p.choice("tones") {
        "shadows" => Tones::Shadows,
        "highlights" => Tones::Highlights,
        _ => Tones::Midtones,
    };
    let push = |name| p.int(name) as f64 / 100.0 * REACH;
    Ok(Box::new(Balance {
        tones,
        push: [push("red"), push("green"), push("blue")],
        keep: p.choice("luminosity") == "keep",
    }))
}

struct Balance {
    tones: Tones,
    push: [f64; 3],
    keep: bool,
}

impl Balance {
    fn weight(&self, l: f64) -> f64 {
        match self.tones {
            Tones::Shadows => (1.0 - l) * (1.0 - l),
            Tones::Midtones => 4.0 * l * (1.0 - l),
            Tones::Highlights => l * l,
        }
    }
}

impl Effect for Balance {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        per_pixel(w, |px| {
            let f = |v: u8| v as f64 / 255.0;
            let (r, g, b) = (f(px[0]), f(px[1]), f(px[2]));
            let l = luma(r, g, b);
            let k = self.weight(l);
            let mut c = [
                (r + k * self.push[0]).clamp(0.0, 1.0),
                (g + k * self.push[1]).clamp(0.0, 1.0),
                (b + k * self.push[2]).clamp(0.0, 1.0),
            ];
            if self.keep {
                let shift = l - luma(c[0], c[1], c[2]);
                c.iter_mut().for_each(|v| *v += shift);
            }
            for i in 0..3 {
                px[i] = to_byte(c[i]);
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn zero_changes_nothing() {
        let e = make(&INFO, "red=0,green=0,blue=0");
        let orig = noise(300, 4);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::U8);
        assert_eq!(b, orig);
    }

    #[test]
    fn tints_its_own_tones_most() {
        // Blue into the shadows: a dark grey turns blue, a light one barely.
        let e = make(&INFO, "tones=shadows,red=0,blue=100,luminosity=change");
        let mut b = vec![30, 30, 30, 230, 230, 230];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert!(b[2] > 100, "{b:?}");
        assert!(b[5] - 230 < 3, "{b:?}");
        assert_eq!((b[0], b[1], b[3], b[4]), (30, 30, 230, 230));
    }

    #[test]
    fn keeping_luminosity_keeps_brightness() {
        let e = make(&INFO, "red=60,blue=-40");
        let mut b = vec![120, 110, 100];
        let before = luma(120.0, 110.0, 100.0);
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert!(b[0] > 120 && b[2] < 100, "{b:?}");
        let after = luma(b[0] as f64, b[1] as f64, b[2] as f64);
        assert!((after - before).abs() < 1.0, "{before} vs {after}");
    }
}
