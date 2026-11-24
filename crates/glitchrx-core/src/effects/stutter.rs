//! `stutter`: repeat the start of each window across it.

use super::{Effect, EffectInfo, Group, PlacementDefaults, Upgrade, Window};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "stutter",
    group: Group::Audio,
    version: 2,
    // Version 1 started with bands; now every effect starts on the whole
    // picture (D-56).
    upgrades: &[Upgrade {
        to: 2,
        fill: &[("hits", "3"), ("len", "5%")],
    }],
    audacity: "Repeat",
    summary: "Copy the first `chunk` of each window over the rest of it, again and again.",
    details: "Whole rows repeat a strip of the picture down the window; part of a row gives a \
              diagonal smear, since each repeat starts a little further along the row. A \
              percentage `chunk` is of the window.",
    params: &[ParamSpec {
        name: "chunk",
        kind: Kind::Size,
        default: "1rows",
        help: "length of the repeated piece (rows, px, bytes, or % of the window)",
    }],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "5%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --stutter chunk=0.9rows,hits=5 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Stutter {
        chunk: p.size("chunk"),
    }))
}

struct Stutter {
    chunk: Bound,
}

impl Effect for Stutter {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let f = w.frame();
        let len = w.bytes.len();
        let c = (self.chunk.of_window(len, w.units) / f).max(1) * f;
        let mut at = c;
        while at < len {
            let n = c.min(len - at);
            w.bytes.copy_within(0..n, at);
            at += n;
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
    fn repeats_the_first_chunk() {
        let e = make(&INFO, "chunk=4");
        let mut b: Vec<u8> = (0..9).collect();
        run(e.as_ref(), &mut b, 3, Encoding::U8);
        assert_eq!(b, vec![0, 1, 2, 3, 0, 1, 2, 3, 0]);
    }

    #[test]
    fn percent_is_of_the_window() {
        let e = make(&INFO, "chunk=50%");
        let mut b: Vec<u8> = (0..6).collect();
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![0, 1, 2, 0, 1, 2]);
    }
}
