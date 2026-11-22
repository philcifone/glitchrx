//! `invert`: Audacity's Invert, `y = -x`.

use super::{Effect, EffectInfo, Filter, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::Params;
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "invert",
    version: 1,
    audacity: "Invert",
    summary: "Flip every sample upside down.",
    details: "With `u8` this is close to a photographic negative. With `s8` and the \
              multi-byte encodings the flip happens around a different point in each byte, \
              so only some channels or some brightness ranges invert.",
    params: &[],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "4",
        len: "4%",
    },
    example: "glitchrx --invert encoding=s16le,hits=8 photo.jpg",
    build,
};

fn build(_: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Invert))
}

struct Invert;

struct Negate;

impl Filter for Negate {
    fn process(&mut self, x: &mut [f64]) {
        x.iter_mut().for_each(|v| *v = -*v);
    }
}

impl Effect for Invert {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        w.filter(|| Negate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn u8_inverts_around_128_and_clips_the_bottom() {
        let e = make(&INFO, "");
        let mut b = [128, 129, 200, 0];
        run(e.as_ref(), &mut b[..3], 1, Encoding::U8);
        assert_eq!(&b[..3], &[128, 127, 56]);
        // 0 is -1.0; +1.0 does not fit in u8 and clips to 255.
        run(e.as_ref(), &mut b[1..4], 1, Encoding::U8);
        assert_eq!(b[3], 255);
    }

    #[test]
    fn twice_is_identity_away_from_the_edge() {
        let e = make(&INFO, "");
        let orig: Vec<u8> = (1..=255).collect();
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 85, Encoding::U8);
        run(e.as_ref(), &mut b, 85, Encoding::U8);
        assert_eq!(b, orig);
    }
}
