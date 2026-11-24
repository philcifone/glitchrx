//! `wahwah`: Audacity's Wahwah, a resonant filter swept by an LFO.

use super::{Effect, EffectInfo, Filter, Group, PlacementDefaults, Window};
use crate::dsp::{Lfo, cos_turns, exp, sin_turns};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "wahwah",
    group: Group::Audio,
    version: 1,
    upgrades: &[],
    audacity: "Wahwah",
    summary: "A resonant filter whose cutoff an LFO sweeps up and down: smears that pulse down the picture.",
    details: "Audacity's wahwah: a two-pole resonant low-pass (a biquad) whose cutoff follows \
              an LFO, from `offset` up to `offset + depth` of the range on an exponential \
              scale. Low cutoffs smear each row sideways; high `resonance` rings, which clips \
              into bright fringes. One LFO `period` is one pulse down the rows. Audacity \
              updates the filter every 30 samples, here every 16.",
    params: &[
        ParamSpec {
            name: "period",
            kind: Kind::Size,
            default: "4rows",
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
            default: "0.7",
            help: "how far the cutoff sweeps",
        },
        ParamSpec {
            name: "resonance",
            kind: Kind::Float {
                min: 0.1,
                max: 10.0,
            },
            default: "2.5",
            help: "peak at the cutoff; high values ring",
        },
        ParamSpec {
            name: "offset",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.3",
            help: "lowest cutoff, as a share of the range",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --wahwah period=8rows,resonance=6 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Wahwah {
        period: p.size("period"),
        start: p.float("start"),
        depth: p.float("depth"),
        resonance: p.float("resonance"),
        offset: p.float("offset"),
    }))
}

struct Wahwah {
    period: Bound,
    start: f64,
    depth: f64,
    resonance: f64,
    offset: f64,
}

struct Wah {
    lfo: Lfo,
    /// Biquad coefficients, divided by `a0`; refreshed when the LFO yields.
    b: [f64; 3],
    a: [f64; 2],
    depth: f64,
    resonance: f64,
    offset: f64,
    /// Last two inputs and outputs of the biquad.
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Filter for Wah {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            if let Some(l) = self.lfo.tick() {
                // Cutoff as a share of the Nyquist frequency: the LFO
                // between `offset` and `offset + depth`, on an exponential
                // scale (Audacity's `exp((f - 1) * 6)`).
                let f = l * self.depth * (1.0 - self.offset) + self.offset;
                let f = exp((f - 1.0) * 6.0);
                // omega = pi * f, so in turns it is f / 2.
                let (sn, cs) = (sin_turns(f * 0.5), cos_turns(f * 0.5));
                let alpha = sn / (2.0 * self.resonance);
                let a0 = 1.0 + alpha;
                self.b = [
                    (1.0 - cs) * 0.5 / a0,
                    (1.0 - cs) / a0,
                    (1.0 - cs) * 0.5 / a0,
                ];
                self.a = [-2.0 * cs / a0, (1.0 - alpha) / a0];
            }
            let ([b0, b1, b2], [a1, a2]) = (self.b, self.a);
            let input = *v;
            let out = b0 * input + b1 * self.x1 + b2 * self.x2 - a1 * self.y1 - a2 * self.y2;
            (self.x2, self.x1) = (self.x1, input);
            (self.y2, self.y1) = (self.y1, out);
            *v = out;
        }
    }
}

impl Effect for Wahwah {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let period = w.samples(w.units.to_raster(w.units.full_bytes(self.period)));
        w.filter(|| Wah {
            lfo: Lfo::new(period, self.start),
            b: [0.0; 3],
            a: [0.0; 2],
            depth: self.depth,
            resonance: self.resonance,
            offset: self.offset,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn passes_a_steady_level() {
        // A low-pass lets a constant through unchanged once it settles
        // (DC gain is 1), so a flat grey stays grey after the start.
        let e = make(&INFO, "");
        let mut b = u8_of(&[0.25; 3000]);
        run(e.as_ref(), &mut b, 50, Encoding::U8);
        let want = u8_of(&[0.25])[0];
        assert!(
            b[2000..].iter().all(|&v| v.abs_diff(want) <= 1),
            "{:?}",
            &b[2000..2010]
        );
    }

    #[test]
    fn smooths_noise() {
        let e = make(&INFO, "depth=0,offset=0.2");
        let orig = noise(3000, 8);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 50, Encoding::U8);
        let roughness =
            |v: &[u8]| -> u64 { v.windows(2).map(|p| p[0].abs_diff(p[1]) as u64).sum() };
        assert!(roughness(&b) < roughness(&orig) / 2);
    }
}
