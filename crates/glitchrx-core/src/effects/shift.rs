//! `shift`: rotate each window, like cutting its end and pasting it first.

use super::{Effect, EffectInfo, PHASE, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "shift",
    version: 1,
    audacity: "cut and paste",
    summary: "Rotate each window forward by `by`; what falls off the end wraps to the start.",
    details: "A fraction of a row slides a band of the picture sideways. `phase` adds bytes \
              to the distance, which moves the colour channels out of step (red into green, \
              green into blue). Edits move whole samples, so with multi-byte encodings the \
              distance is rounded down to a whole sample.",
    params: &[
        ParamSpec {
            name: "by",
            kind: Kind::Size,
            default: "0.25rows",
            help: "distance (rows, px, % or bytes)",
        },
        PHASE,
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "6",
        len: "3%",
    },
    random_within: false,
    example: "glitchrx --shift by=0.5rows,phase=2,hits=12 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Shift {
        by: p.size("by"),
        phase: p.int("phase") as usize,
    }))
}

struct Shift {
    by: Bound,
    phase: usize,
}

impl Effect for Shift {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let (f, n) = (w.frame(), w.frames());
        if n == 0 {
            return Ok(());
        }
        let k = (w.units.offset(self.by, self.phase) / f) % n;
        w.bytes.rotate_right(k * f);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn rotates_by_distance_plus_phase() {
        let e = make(&INFO, "by=1px,phase=1");
        let mut b: Vec<u8> = (0..12).collect();
        run(e.as_ref(), &mut b, 4, Encoding::U8);
        assert_eq!(b, vec![8, 9, 10, 11, 0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn multi_byte_rounds_to_whole_samples() {
        let e = make(&INFO, "by=3,phase=0");
        let mut b: Vec<u8> = (0..6).collect();
        run(e.as_ref(), &mut b, 2, Encoding::S16Be);
        assert_eq!(b, vec![4, 5, 0, 1, 2, 3]);
    }
}
