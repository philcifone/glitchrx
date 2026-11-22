//! `shuffle`: cut blocks out of each window and paste them back in a
//! random order.

use super::{Effect, EffectInfo, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "shuffle",
    version: 1,
    audacity: "cut and paste",
    summary: "Split each window into blocks and swap a share of them at random.",
    details: "Blocks of whole rows rearrange bands of the picture; blocks of part of a row \
              tear it into sideways-displaced strips. A percentage `block` is of the window, \
              not the image. Seeded, so the same seed picks the same blocks.",
    params: &[
        ParamSpec {
            name: "block",
            kind: Kind::Size,
            default: "0.5rows",
            help: "block size (rows, px, bytes, or % of the window)",
        },
        ParamSpec {
            name: "fraction",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "share of the blocks that move",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "4",
        len: "10%",
    },
    random_within: true,
    example: "glitchrx --shuffle block=2rows,fraction=1 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Shuffle {
        block: p.size("block"),
        fraction: p.float("fraction"),
    }))
}

struct Shuffle {
    block: Bound,
    fraction: f64,
}

impl Effect for Shuffle {
    fn run(&self, w: &mut Window<'_>, rng: &mut Rng) -> Result<()> {
        let f = w.frame();
        // Block length in bytes, a whole number of frames.
        let b = (self.block.of_window(w.bytes.len(), w.units) / f).max(1) * f;
        let n = w.bytes.len() / b;
        let mut m = ((n as f64 * self.fraction).round() as usize).min(n);
        // Moving a single block is a no-op; if anything should move, move two.
        if m == 1 && n >= 2 {
            m = 2;
        }
        if m < 2 {
            return Ok(());
        }
        // Partial Fisher-Yates: the first m entries become a uniform random
        // selection of m distinct blocks; then deal them to a shuffled order.
        let mut idx: Vec<usize> = (0..n).collect();
        for i in 0..m {
            let j = i + rng.below((n - i) as u64) as usize;
            idx.swap(i, j);
        }
        let from = &idx[..m];
        let mut to = from.to_vec();
        rng.shuffle(&mut to);
        let copy: Vec<u8> = from
            .iter()
            .flat_map(|&s| w.bytes[s * b..(s + 1) * b].iter().copied())
            .collect();
        for (t, &d) in to.iter().enumerate() {
            w.bytes[d * b..(d + 1) * b].copy_from_slice(&copy[t * b..(t + 1) * b]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn full_shuffle_is_a_permutation_of_blocks() {
        let e = make(&INFO, "block=3,fraction=1");
        let orig: Vec<u8> = (0..30).collect();
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::U8);
        assert_ne!(b, orig);
        let mut blocks: Vec<&[u8]> = b.chunks(3).collect();
        blocks.sort();
        assert_eq!(blocks.concat(), orig, "blocks stay intact");
    }

    #[test]
    fn zero_fraction_is_identity() {
        let e = make(&INFO, "fraction=0");
        let orig = noise(300, 1);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::U8);
        assert_eq!(b, orig);
    }
}
