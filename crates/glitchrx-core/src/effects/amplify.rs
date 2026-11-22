//! `amplify`: Audacity's Amplify, a gain in decibels with clipping.

use super::{Effect, EffectInfo, Filter, PlacementDefaults, Window};
use crate::dsp::db_to_gain;
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "amplify",
    version: 1,
    audacity: "Amplify",
    summary: "Multiply every sample by a gain given in decibels, clipping at the limits.",
    details: "Positive gains push bytes away from the encoding's silence point and clip them: \
              with `u8` that is a harsh contrast boost around mid grey, with `s8` it folds \
              highlights into shadows, with the multi-byte encodings it explodes into noise.",
    params: &[ParamSpec {
        name: "gain",
        kind: Kind::Float {
            min: -40.0,
            max: 40.0,
        },
        default: "6",
        help: "gain in dB (+6 dB doubles, -6 dB halves)",
    }],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "4",
        len: "5%",
    },
    example: "glitchrx --amplify gain=12,hits=6 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Amplify {
        gain: db_to_gain(p.float("gain")),
    }))
}

struct Amplify {
    gain: f64,
}

struct Gain(f64);

impl Filter for Gain {
    fn process(&mut self, x: &mut [f64]) {
        x.iter_mut().for_each(|v| *v *= self.0);
    }
}

impl Effect for Amplify {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        w.filter(|| Gain(self.gain))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn six_db_roughly_doubles_and_clips() {
        let e = make(&INFO, "gain=6");
        let mut b = u8_of(&[0.25, -0.25, 0.75]);
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        // 10^(6/20) = 1.995..., so 0.25 -> 0.4988 (byte 128 + 63.85 -> 192).
        assert_eq!(b, vec![192, 64, 255]);
    }

    #[test]
    fn zero_db_is_identity() {
        let e = make(&INFO, "gain=0");
        let orig = noise(300, 3);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::ULaw);
        assert_eq!(b, orig);
    }
}
