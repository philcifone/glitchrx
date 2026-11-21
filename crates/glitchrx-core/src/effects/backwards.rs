//! Reverse the byte order of each window.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility};
use crate::error::Result;
use crate::params::Params;
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "backwards",
    version: 1,
    summary: "Play each window backwards: reverse its byte order.",
    details: "The audio equivalent of reversing a clip. Reversing twice restores the \
              original, so the effect is its own inverse.",
    reversible: Reversibility::Always,
    params: &[],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "3",
        len: "0.2%",
    },
    example: "glitchrx --backwards len=1k,hits=8 photo.jpg",
    build,
};

fn build(_: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Backwards))
}

struct Backwards;

impl Effect for Backwards {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        buf[w].reverse();
    }

    fn invert(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) -> Result<()> {
        buf[w].reverse();
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
    fn hand_checked_and_round_trips() {
        let e = make(&INFO, "");
        let mut b = vec![1, 2, 3, 4];
        e.apply(&mut b, 1..4, &mut Rng::new(0));
        assert_eq!(b, vec![1, 4, 3, 2]);
        round_trip(e.as_ref(), &noise(300, 4), 0..300);
    }
}
