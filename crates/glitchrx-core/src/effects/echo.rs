//! Feed-forward echo: add quieter, delayed copies of the input to itself.

use std::ops::Range;

use super::{Effect, EffectInfo, PlacementDefaults, Reversibility};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "echo",
    version: 1,
    summary: "Feed-forward echo: each byte gets quieter copies of the bytes `delay`, 2x`delay`, ... before it added on.",
    details: "`y[i] = x[i] + sum over taps k of (q_k * x[i - k*delay]) >> 8`, modulo 256, where `q_k` is \
              `gain^k` in Q8 fixed point. Because the added terms depend only on the input, the inverse \
              subtracts them again walking forward through the window.",
    reversible: Reversibility::Always,
    params: &[
        ParamSpec {
            name: "delay",
            kind: Kind::Int {
                min: 1,
                max: 1 << 24,
            },
            default: "1024",
            help: "distance in bytes between a byte and its echo",
        },
        ParamSpec {
            name: "gain",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "loudness of the first echo, 0..1",
        },
        ParamSpec {
            name: "taps",
            kind: Kind::Int { min: 1, max: 8 },
            default: "1",
            help: "number of echoes; tap k is gain^k loud at k x delay",
        },
    ],
    placement: PlacementDefaults {
        region: "5%:100%",
        hits: "3",
        len: "0.5%",
    },
    example: "glitchrx --echo delay=2048,gain=0.6,taps=2 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let q1 = p.q8("gain");
    // Tap gains as Q8, computed with integer math only: q_k = q_{k-1} * q_1.
    let mut q = Vec::new();
    let mut cur = 256i32;
    for _ in 0..p.int("taps") {
        cur = (cur * q1 + 128) >> 8;
        q.push(cur as u32);
    }
    Ok(Box::new(Echo {
        delay: p.int("delay") as usize,
        q,
    }))
}

struct Echo {
    delay: usize,
    /// Q8 gain per tap (index 0 is tap 1).
    q: Vec<u32>,
}

impl Echo {
    /// Sum of the echo terms for position `i`, reading the *input* values.
    fn echo_at(&self, buf: &[u8], i: usize) -> u8 {
        let mut acc: u32 = 0;
        for (k, &q) in self.q.iter().enumerate() {
            let back = self.delay * (k + 1);
            if back <= i {
                acc = acc.wrapping_add((q * buf[i - back] as u32) >> 8);
            }
        }
        acc as u8
    }
}

impl Effect for Echo {
    fn apply(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) {
        // Walk backwards: y[i] needs x[i - delay], which (if inside the
        // window) must still hold its original value. Going from the end
        // means every earlier byte is untouched when we read it.
        for i in w.rev() {
            let e = self.echo_at(buf, i);
            buf[i] = buf[i].wrapping_add(e);
        }
    }

    fn invert(&self, buf: &mut [u8], w: Range<usize>, _rng: &mut Rng) -> Result<()> {
        // Walk forwards: by the time we reach i, every earlier byte has
        // already been restored to x, so the echo terms are exactly the ones
        // that were added.
        for i in w {
            let e = self.echo_at(buf, i);
            buf[i] = buf[i].wrapping_sub(e);
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
        // delay 2, gain 0.5 (q = 128): y[i] = x[i] + x[i-2]/2 inside 2..6.
        let e = make(&INFO, "delay=2,gain=0.5");
        let mut b = vec![10, 20, 30, 40, 50, 60];
        e.apply(&mut b, 2..6, &mut Rng::new(0));
        assert_eq!(b, vec![10, 20, 35, 50, 65, 80]);
    }

    #[test]
    fn wraps_instead_of_clipping() {
        let e = make(&INFO, "delay=1,gain=1.0");
        let mut b = vec![200, 200];
        e.apply(&mut b, 1..2, &mut Rng::new(0));
        assert_eq!(b, vec![200, 144]); // 400 mod 256
    }

    #[test]
    fn round_trips() {
        let buf = noise(5000, 1);
        for params in ["delay=1", "delay=7,gain=0.9,taps=4", "delay=3000,gain=0.3"] {
            let e = make(&INFO, params);
            let g = round_trip(e.as_ref(), &buf, 100..4800);
            assert_ne!(g, buf, "{params} changed nothing");
        }
    }
}
