//! `reverb`: Freeverb, the algorithm behind Audacity's Reverb.

use super::{Effect, EffectInfo, Filter, PHASE, PlacementDefaults, Window};
use crate::dsp::{Allpass, Comb};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "reverb",
    version: 1,
    audacity: "Reverb",
    summary: "Freeverb: eight damped feedback combs in parallel, then four allpass diffusers.",
    details: "Smears the picture into a haze of overlapping ghosts. `size` is the longest comb \
              delay; the others keep Freeverb's ratios to it, so the ghosts never line up. \
              `room` sets how long the tail rings, `damping` how fast fine detail dies out of it.",
    params: &[
        ParamSpec {
            name: "size",
            kind: Kind::Size,
            default: "0.5rows",
            help: "longest comb delay (rows, px, % or bytes)",
        },
        PHASE,
        ParamSpec {
            name: "room",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.7",
            help: "length of the tail",
        },
        ParamSpec {
            name: "damping",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "how fast detail fades from the tail",
        },
        ParamSpec {
            name: "wet",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.33",
            help: "level of the reverb",
        },
        ParamSpec {
            name: "dry",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "level of the original (0.5 = unchanged)",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --reverb room=0.9,wet=0.5 photo.jpg",
    build,
};

/// Freeverb's tuning, in samples at 44.1 kHz.
const COMBS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASSES: [usize; 4] = [556, 441, 341, 225];
/// Freeverb's scaling constants.
const FIXED_GAIN: f64 = 0.015;
const SCALE_WET: f64 = 3.0;
const SCALE_DRY: f64 = 2.0;
const SCALE_ROOM: f64 = 0.28;
const OFFSET_ROOM: f64 = 0.7;
const SCALE_DAMP: f64 = 0.4;

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Reverb {
        size: p.size("size"),
        phase: p.int("phase") as usize,
        room: p.float("room"),
        damping: p.float("damping"),
        wet: p.float("wet"),
        dry: p.float("dry"),
    }))
}

struct Reverb {
    size: Bound,
    phase: usize,
    room: f64,
    damping: f64,
    wet: f64,
    dry: f64,
}

struct Freeverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
    wet: f64,
    dry: f64,
}

impl Filter for Freeverb {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            let input = *v * FIXED_GAIN;
            let mut out = 0.0;
            for c in &mut self.combs {
                out += c.process(input);
            }
            for a in &mut self.allpasses {
                out = a.process(out);
            }
            *v = out * self.wet + *v * self.dry;
        }
    }
}

impl Effect for Reverb {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        // Every delay keeps Freeverb's ratio to the longest comb. They are
        // worked out in full-resolution bytes and only then mapped into this
        // raster, so each one rotates the colours the same way in a preview
        // as in the export. Capped at the window (longer would be silent).
        let longest = w.units.full_bytes(self.size) + self.phase;
        let frames = w.frames().max(1);
        let lens = |bases: &[usize]| -> Vec<usize> {
            bases
                .iter()
                .map(|&b| {
                    w.samples(w.units.to_raster(b * longest / COMBS[7]))
                        .min(frames)
                })
                .collect()
        };
        let (comb_lens, allpass_lens) = (lens(&COMBS), lens(&ALLPASSES));
        let feedback = self.room * SCALE_ROOM + OFFSET_ROOM;
        let damp = self.damping * SCALE_DAMP;
        let (wet, dry) = (self.wet * SCALE_WET, self.dry * SCALE_DRY);
        w.filter(|| Freeverb {
            combs: comb_lens
                .iter()
                .map(|&n| Comb::new(n, feedback, damp))
                .collect(),
            allpasses: allpass_lens.iter().map(|&n| Allpass::new(n)).collect(),
            wet,
            dry,
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
        let e = make(&INFO, "wet=0,dry=0.5");
        let orig = noise(3000, 2);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 100, Encoding::U8);
        assert_eq!(b, orig);
    }

    #[test]
    fn impulse_spreads_into_a_tail() {
        let e = make(&INFO, "size=1617,phase=0,wet=1,dry=0");
        let mut b = vec![128u8; 30_000];
        b[0] = 255;
        run(e.as_ref(), &mut b, 10_000, Encoding::U8);
        // Nothing before the shortest allpass-delayed comb output...
        assert!(b[..225].iter().all(|&v| v == 128));
        // ...and many changed samples after it.
        let touched = b.iter().filter(|&&v| v != 128).count();
        assert!(touched > 100, "{touched}");
    }
}
