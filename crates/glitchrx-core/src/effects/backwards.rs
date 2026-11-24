//! `backwards`: Audacity's Reverse.

use super::{Effect, EffectInfo, Group, PlacementDefaults, Upgrade, Window};
use crate::error::Result;
use crate::params::Params;
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "backwards",
    group: Group::Data,
    version: 2,
    // Version 1 started with bands; now every effect starts on the whole
    // picture (D-56).
    upgrades: &[Upgrade {
        to: 2,
        fill: &[("hits", "6"), ("len", "2%")],
    }],
    audacity: "Reverse",
    summary: "Play each window backwards.",
    details: "Reverses the order of the samples (not the bytes inside a sample). A window \
              covering whole rows mirrors them left to right and flips their order; with \
              `u8` the colour channels also swap, red with blue.",
    params: &[],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "2%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --backwards hits=10,len=3rows photo.jpg",
    build,
};

fn build(_: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Backwards))
}

struct Backwards;

impl Effect for Backwards {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let f = w.frame();
        // Reversing every byte also reverses the bytes inside each frame;
        // reversing each frame again puts those back in order. Two cheap
        // linear passes instead of a frame-by-frame swap loop.
        w.bytes.reverse();
        if f > 1 {
            w.bytes.chunks_exact_mut(f).for_each(<[u8]>::reverse);
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
    fn reverses_samples_not_bytes() {
        let e = make(&INFO, "");
        let mut b = vec![1, 2, 3, 4, 5, 6];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![6, 5, 4, 3, 2, 1]);
        let mut b = vec![1, 2, 3, 4, 5, 6];
        run(e.as_ref(), &mut b, 2, Encoding::S16Le);
        assert_eq!(b, vec![5, 6, 3, 4, 1, 2]);
    }
}
