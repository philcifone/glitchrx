//! Stutter: repeat the start of each window over the rest of it.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility, not_reversible};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "stutter",
    version: 1,
    summary: "Copy the first `chunk` bytes of each window and repeat them until the window is full.",
    details: "Like a skipping CD. The bytes that get overwritten are lost, so the effect cannot \
              be reversed. The file length never changes: the repeats replace data rather than \
              insert it.",
    reversible: Reversibility::Never,
    params: &[ParamSpec {
        name: "chunk",
        kind: Kind::Int {
            min: 1,
            max: 1 << 24,
        },
        default: "256",
        help: "length of the repeated chunk in bytes",
    }],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "3",
        len: "0.5%",
    },
    example: "glitchrx --stutter chunk=64,len=8k photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Stutter {
        chunk: p.int("chunk") as usize,
    }))
}

struct Stutter {
    chunk: usize,
}

impl Effect for Stutter {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        let win = &mut buf[w];
        let c = self.chunk.min(win.len());
        for i in c..win.len() {
            win[i] = win[i % c];
        }
    }

    fn invert(&self, _: &mut [u8], _: Range<usize>, _: &mut Rng) -> Result<()> {
        not_reversible("stutter")
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
        let e = make(&INFO, "chunk=2");
        let mut b = vec![9, 1, 2, 3, 4, 5, 6];
        e.apply(&mut b, 1..6, &mut Rng::new(0));
        assert_eq!(b, vec![9, 1, 2, 1, 2, 1, 6]);
    }
}
