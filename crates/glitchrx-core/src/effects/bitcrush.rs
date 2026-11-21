//! Bit-crush: throw away low-order bits.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility, not_reversible};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "bitcrush",
    version: 1,
    summary: "Keep only the top `bits` bits of every byte; the rest become zero.",
    details: "The audio bit-crusher reduces sample resolution. On compressed data it collapses \
              many different codes onto the same values. The discarded bits are gone, so the \
              effect cannot be reversed.",
    reversible: Reversibility::Never,
    params: &[ParamSpec {
        name: "bits",
        kind: Kind::Int { min: 1, max: 7 },
        default: "4",
        help: "bits kept per byte (1 = harshest)",
    }],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "3",
        len: "0.3%",
    },
    example: "glitchrx --bitcrush bits=2,hits=5 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let bits = p.int("bits") as u32;
    Ok(Box::new(Bitcrush {
        mask: 0xFFu8 << (8 - bits),
    }))
}

struct Bitcrush {
    mask: u8,
}

impl Effect for Bitcrush {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        for b in &mut buf[w] {
            *b &= self.mask;
        }
    }

    fn invert(&self, _: &mut [u8], _: Range<usize>, _: &mut Rng) -> Result<()> {
        not_reversible("bitcrush")
    }

    fn reversible(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;

    #[test]
    fn hand_checked() {
        let e = make(&INFO, "bits=4");
        let mut b = vec![0xFF, 0x1F, 0xAB];
        e.apply(&mut b, 0..2, &mut Rng::new(0));
        assert_eq!(b, vec![0xF0, 0x10, 0xAB]);
        assert!(!e.reversible());
    }
}
