//! Shuffle blocks of bytes within each window.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "shuffle",
    version: 1,
    summary: "Cut each window into `block`-byte blocks and swap a `fraction` of them around.",
    details: "Blocks are chosen and permuted with the seeded RNG, so the inverse simply \
              regenerates the same permutation and moves every block back. Any partial \
              block at the end of a window is left in place.",
    reversible: Reversibility::Always,
    params: &[
        ParamSpec {
            name: "block",
            kind: Kind::Int {
                min: 1,
                max: 1 << 24,
            },
            default: "256",
            help: "block size in bytes",
        },
        ParamSpec {
            name: "fraction",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "share of blocks that move, 0..1",
        },
    ],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "2",
        len: "2%",
    },
    example: "glitchrx --shuffle block=1024,fraction=0.8 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Shuffle {
        block: p.int("block") as usize,
        fraction_q8: p.q8("fraction") as u64,
    }))
}

struct Shuffle {
    block: usize,
    fraction_q8: u64,
}

impl Shuffle {
    /// Returns `(from, to)` block indices: block `from[t]` moves to `to[t]`.
    fn plan(&self, window_len: usize, rng: &mut Rng) -> (Vec<usize>, Vec<usize>) {
        let n = window_len / self.block;
        let m = ((n as u64 * self.fraction_q8 + 128) >> 8).min(n as u64) as usize;
        // Partial Fisher-Yates: the first m entries become a uniform random
        // selection of m distinct blocks.
        let mut idx: Vec<usize> = (0..n).collect();
        for i in 0..m {
            let j = i + rng.below((n - i) as u64) as usize;
            idx.swap(i, j);
        }
        let from: Vec<usize> = idx[..m].to_vec();
        let mut to = from.clone();
        rng.shuffle(&mut to);
        (from, to)
    }

    fn move_blocks(&self, win: &mut [u8], src: &[usize], dst: &[usize]) {
        let copy = win.to_vec();
        let b = self.block;
        for (&s, &d) in src.iter().zip(dst) {
            win[d * b..(d + 1) * b].copy_from_slice(&copy[s * b..(s + 1) * b]);
        }
    }
}

impl Effect for Shuffle {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, rng: &mut Rng) {
        let (from, to) = self.plan(w.len(), rng);
        self.move_blocks(&mut buf[w], &from, &to);
    }

    fn invert(&self, buf: &mut [u8], w: Range<usize>, rng: &mut Rng) -> Result<()> {
        let (from, to) = self.plan(w.len(), rng);
        self.move_blocks(&mut buf[w], &to, &from);
        Ok(())
    }

    fn reversible(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;

    #[test]
    fn moves_whole_blocks_only() {
        let e = make(&INFO, "block=4,fraction=1.0");
        let orig: Vec<u8> = (0..18).collect(); // 4 blocks + 2 tail bytes
        let mut b = orig.clone();
        e.apply(&mut b, 0..18, &mut Rng::new(5));
        assert_eq!(&b[16..], &orig[16..], "tail must not move");
        let mut blocks: Vec<&[u8]> = b[..16].chunks(4).collect();
        blocks.sort();
        let expect: Vec<&[u8]> = orig[..16].chunks(4).collect();
        assert_eq!(blocks, expect, "same blocks, possibly reordered");
    }

    #[test]
    fn round_trips() {
        let buf = noise(10_000, 5);
        for params in ["block=1,fraction=1.0", "block=256", "block=7,fraction=0.3"] {
            let g = round_trip(make(&INFO, params).as_ref(), &buf, 33..9_900);
            assert_ne!(g, buf, "{params} changed nothing");
        }
    }
}
