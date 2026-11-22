//! Comb-filter reverb: three feedback echoes in series.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "reverb",
    version: 1,
    summary: "Comb-filter reverb: three feedback echoes in series, with delays `delay`, 1.37x and 1.71x `delay`.",
    details: "Each comb computes `y[i] = x[i] + (q * y[i - d]) >> 8` modulo 256, feeding its own output back, \
              so echoes repeat and decay. A true audio reverb is far more complex; this is a deliberately \
              simple approximation. Interesting property: the inverse, `x[i] = y[i] - (q * y[i - d]) >> 8`, \
              only needs output values, so it is exact even though the forward pass is recursive.",
    reversible: Reversibility::Always,
    params: &[
        ParamSpec {
            name: "delay",
            kind: Kind::Int {
                min: 1,
                max: 1 << 24,
            },
            default: "512",
            help: "delay of the first comb in bytes; the others are 1.37x and 1.71x",
        },
        ParamSpec {
            name: "feedback",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.4",
            help: "how much of each comb's output is fed back, 0..1",
        },
    ],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "2",
        len: "1%",
    },
    example: "glitchrx --reverb delay=800,feedback=0.6 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let d = p.int("delay") as u64;
    // 1.37 and 1.71 in integer math (rounded): roughly coprime ratios so the
    // three combs do not reinforce each other on the same bytes.
    let delays = [d, (d * 137 + 50) / 100, (d * 171 + 50) / 100].map(|v| v.max(1) as usize);
    Ok(Box::new(Reverb {
        delays,
        q: p.q8("feedback") as u32,
    }))
}

struct Reverb {
    delays: [usize; 3],
    q: u32,
}

impl Effect for Reverb {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        for &d in &self.delays {
            // Forward, in place: buf[i - d] already holds this comb's output
            // y[i - d] (or an untouched byte before the window).
            for i in w.clone() {
                if i >= d {
                    let fb = ((self.q * buf[i - d] as u32) >> 8) as u8;
                    buf[i] = buf[i].wrapping_add(fb);
                }
            }
        }
    }

    fn invert(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) -> Result<()> {
        // Undo the combs in reverse order. Within a comb walk backwards so
        // that buf[i - d] is still the comb's output y[i - d] when we read it.
        for &d in self.delays.iter().rev() {
            for i in w.clone().rev() {
                if i >= d {
                    let fb = ((self.q * buf[i - d] as u32) >> 8) as u8;
                    buf[i] = buf[i].wrapping_sub(fb);
                }
            }
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
    fn hand_checked_single_comb_behaviour() {
        // delay=1 gives comb delays 1, 1 and 2 (1.37 and 1.71 rounded). With
        // a one-byte window at index 1, combs 1 and 2 each add half of the
        // byte before it; comb 3 would reach index -1, so it does nothing.
        let e = make(&INFO, "delay=1,feedback=0.5");
        let mut b = vec![100, 10];
        e.apply(&mut b, 1..2, &mut Rng::new(0));
        assert_eq!(b, vec![100, 10 + 50 + 50]);
    }

    #[test]
    fn round_trips() {
        let buf = noise(6000, 2);
        for params in [
            "delay=1,feedback=1.0",
            "delay=50,feedback=0.7",
            "delay=4000",
        ] {
            let e = make(&INFO, params);
            let g = round_trip(e.as_ref(), &buf, 10..6000);
            assert_ne!(g, buf, "{params} changed nothing");
        }
    }
}
