//! `phaser`: Audacity's Phaser, a chain of all-pass stages swept by an LFO.

use super::{Effect, EffectInfo, Filter, Group, PlacementDefaults, Window};
use crate::dsp::{Lfo, exp};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "phaser",
    group: Group::Audio,
    version: 1,
    upgrades: &[],
    audacity: "Phaser",
    summary: "All-pass stages swept by a slow oscillator, mixed with the input: wavy, combed bands.",
    details: "Audacity's phaser: `stages` first-order all-pass filters in series, their \
              coefficient swept between 1 and `1 - depth` by an LFO, with `feedback` from the \
              output back into the input, then mixed with the dry signal. In a picture the \
              sweep runs down the rows, so one LFO `period` is one wave of the banding. Audacity \
              updates the sweep every 20 samples, here every 16.",
    params: &[
        ParamSpec {
            name: "stages",
            kind: Kind::Int { min: 2, max: 24 },
            default: "4",
            help: "all-pass stages; more gives more, finer notches",
        },
        ParamSpec {
            name: "period",
            kind: Kind::Size,
            default: "6rows",
            help: "length of one LFO sweep (rows, px, % or bytes)",
        },
        ParamSpec {
            name: "start",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0",
            help: "where in its sweep the LFO starts, in turns (0.5 is half way)",
        },
        ParamSpec {
            name: "depth",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.4",
            help: "how far the sweep reaches",
        },
        ParamSpec {
            name: "feedback",
            kind: Kind::Float {
                min: -0.99,
                max: 0.99,
            },
            default: "0.5",
            help: "output fed back into the input; negative inverts it",
        },
        ParamSpec {
            name: "mix",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "share of the phased signal in the output (Audacity's dry/wet)",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: true,
    example: "glitchrx --phaser stages=8,period=3rows,feedback=0.8 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Phaser {
        stages: p.int("stages") as usize,
        period: p.size("period"),
        start: p.float("start"),
        depth: p.float("depth"),
        feedback: p.float("feedback"),
        mix: p.float("mix"),
    }))
}

struct Phaser {
    stages: usize,
    period: Bound,
    start: f64,
    depth: f64,
    feedback: f64,
    mix: f64,
}

/// Audacity's LFO shaping constant: the sweep lingers near the top.
const SHAPE: f64 = 4.0;

struct Phase {
    lfo: Lfo,
    /// All-pass coefficient, refreshed when the LFO yields.
    g: f64,
    old: Vec<f64>,
    fb_out: f64,
    depth: f64,
    feedback: f64,
    mix: f64,
    /// `e^SHAPE - 1`, the shaping curve's normaliser.
    norm: f64,
}

impl Filter for Phase {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            let input = *v;
            // All-pass coefficient: the shaped LFO, scaled down from 1 by
            // `depth`, recomputed when the LFO moves on.
            if let Some(l) = self.lfo.tick() {
                let sweep = (exp(l * SHAPE) - 1.0) / self.norm;
                self.g = 1.0 - sweep * self.depth;
            }
            let g = self.g;
            let mut m = input + self.fb_out * self.feedback;
            for s in &mut self.old {
                let tmp = *s;
                *s = g * tmp + m;
                m = tmp - g * *s;
            }
            self.fb_out = m;
            *v = m * self.mix + input * (1.0 - self.mix);
        }
    }
}

impl Effect for Phaser {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        // The period is set at full resolution and mapped like any distance,
        // so a preview shows the same number of waves.
        let period = w.samples(w.units.to_raster(w.units.full_bytes(self.period)));
        let norm = exp(SHAPE) - 1.0;
        w.filter(|| Phase {
            lfo: Lfo::new(period, self.start),
            g: 1.0,
            old: vec![0.0; self.stages],
            fb_out: 0.0,
            depth: self.depth,
            feedback: self.feedback,
            mix: self.mix,
            norm,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn dry_only_is_identity() {
        let e = make(&INFO, "mix=0");
        let orig = noise(600, 2);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::S16Le);
        assert_eq!(b, orig);
    }

    #[test]
    fn it_changes_the_picture_and_stays_finite() {
        let e = make(&INFO, "stages=8,period=2rows,feedback=0.95,mix=1");
        let orig = noise(3000, 5);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 20, Encoding::U8);
        assert_ne!(b, orig);
    }

    #[test]
    fn deterministic() {
        let e = make(&INFO, "");
        let orig = noise(3000, 6);
        let (mut a, mut b) = (orig.clone(), orig);
        run(e.as_ref(), &mut a, 20, Encoding::ULaw);
        run(e.as_ref(), &mut b, 20, Encoding::ULaw);
        assert_eq!(a, b);
    }
}
