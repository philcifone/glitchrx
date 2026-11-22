//! `speed`: Audacity's Change Speed, fitted back into the window.

use super::{BLOCK, Effect, EffectInfo, PlacementDefaults, Window};
use crate::error::{Error, Result};
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;
use crate::samples::Encoding;

pub static INFO: EffectInfo = EffectInfo {
    id: "speed",
    version: 1,
    audacity: "Change Speed",
    summary: "Play each window faster or slower, wrapping around to keep its length: shears and stretches.",
    details: "Output sample `i` reads the input at `i * factor`, between samples by linear \
              interpolation, wrapping back to the start of the window. Faster than 1 squeezes \
              the window's rows and repeats it, and since a squeezed row no longer lines up \
              with the picture's rows, everything leans into a diagonal shear. Slower than 1 \
              stretches the start of the window across all of it. With `step=pixel` (the \
              default) whole pixels are resampled, so colours hold. `step=sample` resamples \
              each sample like Audacity, which drifts across the R, G and B bytes and blends \
              them into grey stripes.",
    params: &[
        ParamSpec {
            name: "factor",
            kind: Kind::Float {
                min: 0.25,
                max: 4.0,
            },
            default: "1.5",
            help: "playback speed; above 1 squeezes and repeats, below 1 stretches",
        },
        ParamSpec {
            name: "step",
            kind: Kind::Choice(&["pixel", "sample"]),
            default: "pixel",
            help: "resample whole pixels (colours hold) or single samples (grey stripes)",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "4",
        len: "8%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --speed factor=1.1,hits=6 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Speed {
        factor: p.float("factor"),
        pixels: p.choice("step") == "pixel",
    }))
}

struct Speed {
    factor: f64,
    pixels: bool,
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Read positions are fixed point with this many fraction bits, so stepping
/// through millions of samples is exact integer arithmetic (no drift, same
/// result everywhere).
const FRAC_BITS: u32 = 32;

impl Effect for Speed {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        // Resample in units ("groups") of whole frames. With `step=pixel` a
        // group is also whole pixels: the least common multiple of the frame
        // and 3 bytes. Each sample slot of a group is interpolated from the
        // same slot in neighbouring groups, so red stays red.
        let (enc, width, frame) = (w.encoding, w.encoding.width(), w.frame());
        let group = if self.pixels {
            frame / gcd(frame, 3) * 3
        } else {
            frame
        };
        let groups = (w.bytes.len() / group) as u128;
        if groups < 2 {
            return Ok(());
        }
        let step = (self.factor * (1u64 << FRAC_BITS) as f64).round() as u128;
        let one = 1u128 << FRAC_BITS;
        // Read from an untouched copy: output and input overlap.
        let src = w.bytes.to_vec();
        let read = |enc: Encoding, at: usize| enc.read(&src[at..at + width]);
        for i in 0..groups {
            if i % BLOCK as u128 == 0 && (w.cancel)() {
                return Err(Error::Cancelled);
            }
            let pos = i * step;
            let g0 = ((pos >> FRAC_BITS) % groups) as usize;
            let g1 = (g0 + 1) % groups as usize;
            let frac = (pos & (one - 1)) as f64 / one as f64;
            for slot in (0..group).step_by(width) {
                let a = read(enc, g0 * group + slot);
                let b = read(enc, g1 * group + slot);
                let at = i as usize * group + slot;
                enc.write(a + (b - a) * frac, &mut w.bytes[at..at + width]);
            }
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
    fn double_speed_takes_every_other_sample_and_wraps() {
        let e = make(&INFO, "factor=2,step=sample");
        let mut b: Vec<u8> = (0..6).map(|v| v * 10).collect();
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![0, 20, 40, 0, 20, 40]);
    }

    #[test]
    fn half_speed_interpolates() {
        let e = make(&INFO, "factor=0.5,step=sample");
        let mut b: Vec<u8> = vec![0, 100, 200, 0, 0, 0];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![0, 50, 100, 150, 200, 100]);
    }

    #[test]
    fn pixel_step_keeps_colours_together() {
        // Two pixels per row: red, green, blue, white. Double speed takes
        // every other pixel, whole: red, blue, red, blue.
        let (r, g, b, wh) = ([255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 255]);
        let mut px: Vec<u8> = [r, g, b, wh].concat();
        let e = make(&INFO, "factor=2");
        run(e.as_ref(), &mut px, 2, Encoding::U8);
        assert_eq!(px, [r, b, r, b].concat());
    }

    #[test]
    fn unit_speed_is_identity() {
        let e = make(&INFO, "factor=1");
        let orig = noise(600, 4);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::S24Be);
        assert_eq!(b, orig);
        let e = make(&INFO, "factor=1,step=sample");
        let orig = noise(600, 4);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::S24Be);
        assert_eq!(b, orig);
    }
}
