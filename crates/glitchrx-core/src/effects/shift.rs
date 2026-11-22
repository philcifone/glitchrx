//! Circular shift (rotation) of each window.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "shift",
    version: 1,
    summary: "Rotate each window by `by` bytes; bytes pushed off the end wrap to the start.",
    details: "Moves a stretch of compressed data sideways. Typically shows up as blocks displaced \
              horizontally and colour shifts from the point of the hit onwards.",
    reversible: Reversibility::Always,
    params: &[ParamSpec {
        name: "by",
        kind: Kind::Int {
            min: -(1 << 24),
            max: 1 << 24,
        },
        default: "37",
        help: "bytes to rotate by; negative rotates the other way",
    }],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "4",
        len: "1%",
    },
    example: "glitchrx --shift by=200,hits=6 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Shift { by: p.int("by") }))
}

struct Shift {
    by: i64,
}

impl Shift {
    /// Right-rotation amount for a window of length `n`, in `0..n`.
    fn amount(&self, n: usize) -> usize {
        self.by.rem_euclid(n as i64) as usize
    }
}

impl Effect for Shift {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        let win = &mut buf[w];
        if !win.is_empty() {
            let k = self.amount(win.len());
            win.rotate_right(k);
        }
    }

    fn invert(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) -> Result<()> {
        let win = &mut buf[w];
        if !win.is_empty() {
            let k = self.amount(win.len());
            win.rotate_left(k);
        }
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
    fn hand_checked() {
        let e = make(&INFO, "by=1");
        let mut b = vec![1, 2, 3, 4, 5];
        e.apply(&mut b, 1..4, &mut Rng::new(0));
        assert_eq!(b, vec![1, 4, 2, 3, 5]);
        let e = make(&INFO, "by=-1");
        let mut b = vec![1, 2, 3, 4, 5];
        e.apply(&mut b, 1..4, &mut Rng::new(0));
        assert_eq!(b, vec![1, 3, 4, 2, 5]);
    }

    #[test]
    fn round_trips() {
        let buf = noise(1000, 3);
        for params in ["by=1", "by=-999", "by=123456"] {
            round_trip(make(&INFO, params).as_ref(), &buf, 5..990);
        }
    }
}
