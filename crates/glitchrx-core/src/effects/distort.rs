//! `distort`: waveshaping, after Audacity's Distortion.

use super::{Effect, EffectInfo, Filter, Group, PlacementDefaults, Window};
use crate::dsp::db_to_gain;
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "distort",
    group: Group::Audio,
    version: 1,
    upgrades: &[],
    audacity: "Distortion",
    summary: "Drive the samples into a curve: hard clipping, soft saturation, or folding back.",
    details: "Each sample is multiplied by `drive` (dB) and bent by a curve. `hard` clips at \
              full scale, like amplify. `soft` rounds off towards full scale (a cubic curve), \
              for posterized but smooth tones. `fold` reflects anything past full scale back \
              in, again and again, so bright areas turn into stacked bands of inverted \
              contour. `mix` blends with the input.",
    params: &[
        ParamSpec {
            name: "curve",
            kind: Kind::Choice(&["fold", "soft", "hard"]),
            default: "fold",
            help: "shape of the distortion",
        },
        ParamSpec {
            name: "drive",
            kind: Kind::Float {
                min: 0.0,
                max: 48.0,
            },
            default: "12",
            help: "gain into the curve, in dB",
        },
        ParamSpec {
            name: "mix",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "1",
            help: "share of the distorted signal in the output",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    strokes: false,
    example: "glitchrx --distort curve=fold,drive=18 photo.jpg",
    build,
};

#[derive(Clone, Copy)]
enum Curve {
    Hard,
    Soft,
    Fold,
}

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let curve = match p.choice("curve") {
        "hard" => Curve::Hard,
        "soft" => Curve::Soft,
        _ => Curve::Fold,
    };
    Ok(Box::new(Distort {
        curve,
        drive: db_to_gain(p.float("drive")),
        mix: p.float("mix"),
    }))
}

struct Distort {
    curve: Curve,
    drive: f64,
    mix: f64,
}

impl Curve {
    fn apply(self, v: f64) -> f64 {
        match self {
            Curve::Hard => v.clamp(-1.0, 1.0),
            // 1.5v - 0.5v^3 meets full scale with zero slope at |v| = 1.
            Curve::Soft => {
                let c = v.clamp(-1.0, 1.0);
                1.5 * c - 0.5 * c * c * c
            }
            // Reflect into -1..1: a triangle wave of period 4. `floor` is an
            // exact IEEE operation, so this stays deterministic.
            Curve::Fold => {
                let t = (v + 1.0) / 4.0;
                let r = (t - t.floor()) * 4.0; // 0 .. 4
                if r <= 2.0 { r - 1.0 } else { 3.0 - r }
            }
        }
    }
}

struct Shape {
    curve: Curve,
    drive: f64,
    mix: f64,
}

impl Filter for Shape {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            let y = self.curve.apply(*v * self.drive);
            *v = y * self.mix + *v * (1.0 - self.mix);
        }
    }
}

impl Effect for Distort {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        w.filter(|| Shape {
            curve: self.curve,
            drive: self.drive,
            mix: self.mix,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn curves() {
        assert_eq!(Curve::Hard.apply(1.7), 1.0);
        assert_eq!(Curve::Soft.apply(0.0), 0.0);
        assert_eq!(Curve::Soft.apply(3.0), 1.0);
        assert!((Curve::Soft.apply(0.5) - 0.6875).abs() < 1e-12);
        // Folding: 1.5 reflects to 0.5, 2.5 to -0.5, -1.5 to -0.5.
        assert!((Curve::Fold.apply(1.5) - 0.5).abs() < 1e-12);
        assert!((Curve::Fold.apply(2.5) + 0.5).abs() < 1e-12);
        assert!((Curve::Fold.apply(-1.5) + 0.5).abs() < 1e-12);
        assert!((Curve::Fold.apply(0.3) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn no_drive_no_change_below_full_scale() {
        for curve in ["fold", "hard"] {
            let e = make(&INFO, &format!("curve={curve},drive=0"));
            let orig = noise(600, 3);
            let mut b = orig.clone();
            run(e.as_ref(), &mut b, 10, Encoding::S16Le);
            assert_eq!(b, orig, "{curve}");
        }
    }

    #[test]
    fn fold_turns_highlights_dark() {
        let e = make(&INFO, "curve=fold,drive=6");
        let mut b = u8_of(&[0.9]);
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        // 0.9 * 1.995 = 1.796, folded to 0.204: a near-white turns mid grey.
        assert!(b[0] < 170 && b[0] > 140, "{}", b[0]);
    }
}
