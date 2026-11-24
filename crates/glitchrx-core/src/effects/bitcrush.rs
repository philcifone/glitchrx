//! `bitcrush`: keep only the top bits of each sample.

use super::{Effect, EffectInfo, Filter, Group, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "bitcrush",
    group: Group::Data,
    version: 1,
    upgrades: &[],
    audacity: "a bitcrusher (Audacity has none built in; the usual Nyquist plug-ins)",
    summary: "Reduce every sample to `bits` bits of resolution.",
    details: "Posterizes. With `u8` it is plain colour banding; with the multi-byte encodings \
              the lower bytes of each sample are wiped, so whole channels drop to flat values \
              while the others keep their detail.",
    params: &[ParamSpec {
        name: "bits",
        kind: Kind::Int { min: 1, max: 16 },
        default: "3",
        help: "bits kept per sample",
    }],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    strokes: false,
    example: "glitchrx --bitcrush bits=2,encoding=s16be photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Bitcrush {
        bits: p.int("bits") as u32,
    }))
}

struct Bitcrush {
    bits: u32,
}

/// Round each sample down to a multiple of `1 / steps`.
struct Crush {
    steps: f64,
}

impl Filter for Crush {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            *v = (*v * self.steps).floor() / self.steps;
        }
    }
}

impl Effect for Bitcrush {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        // Samples span -1..1, so b bits give 2^(b-1) steps per unit.
        let steps = (1u32 << (self.bits - 1)) as f64;
        w.filter(|| Crush { steps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn two_bits_leave_four_levels() {
        let e = make(&INFO, "bits=2");
        let mut b: Vec<u8> = (0..=255).collect();
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        let mut levels = b.clone();
        levels.dedup();
        assert_eq!(levels, vec![0, 64, 128, 192]);
    }

    #[test]
    fn eight_bits_of_u8_is_identity() {
        let e = make(&INFO, "bits=8");
        let orig = noise(99, 5);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 33, Encoding::U8);
        assert_eq!(b, orig);
    }
}
