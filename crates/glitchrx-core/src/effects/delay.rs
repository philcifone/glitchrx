//! `delay`: feed-forward taps, after Audacity's Nyquist Delay.

use super::{Effect, EffectInfo, Filter, Group, PHASE, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "delay",
    group: Group::Audio,
    version: 1,
    upgrades: &[],
    audacity: "Delay",
    summary: "A fixed number of repeats of the input, each `decay` times quieter than the last.",
    details: "`y[n] = x[n] + decay * x[n - d] + decay^2 * x[n - 2d] + ...` for `taps` repeats. \
              Unlike `echo`, repeats come from the input, not the output, so they stop after \
              `taps` copies: crisper, countable ghosts.",
    params: &[
        ParamSpec {
            name: "delay",
            kind: Kind::Size,
            default: "0.33rows",
            help: "distance between repeats (rows, px, % or bytes)",
        },
        PHASE,
        ParamSpec {
            name: "decay",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "loudness of each repeat relative to the one before",
        },
        ParamSpec {
            name: "taps",
            kind: Kind::Int { min: 1, max: 16 },
            default: "3",
            help: "number of repeats",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --delay delay=2rows,taps=4 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Delay {
        delay: p.size("delay"),
        phase: p.int("phase") as usize,
        decay: p.float("decay"),
        taps: p.int("taps") as usize,
    }))
}

struct Delay {
    delay: Bound,
    phase: usize,
    decay: f64,
    taps: usize,
}

/// The input history, `taps * d` samples long.
///
/// Stored as `f32`, which halves the memory and is still exact: every
/// encoding's samples are at most 24-bit integers divided by a power of two,
/// and an f32 holds 24 significant bits.
struct Taps {
    hist: Vec<f32>,
    pos: usize,
    d: usize,
    gains: Vec<f64>,
}

impl Filter for Taps {
    fn process(&mut self, x: &mut [f64]) {
        let len = self.hist.len();
        for v in x {
            let mut y = *v;
            for (k, g) in self.gains.iter().enumerate() {
                // Tap k+1 reads the input (k+1)*d samples ago. Untouched
                // history is zero, so early taps read silence.
                let back = (k + 1) * self.d;
                y += g * self.hist[(self.pos + len - back) % len] as f64;
            }
            self.hist[self.pos] = *v as f32;
            self.pos = (self.pos + 1) % len;
            *v = y;
        }
    }
}

impl Effect for Delay {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let d = w.samples(w.units.offset(self.delay, self.phase));
        // Taps that land past the end of the window are never heard.
        let taps = self.taps.min(w.frames().saturating_sub(1) / d);
        if taps == 0 {
            return Ok(());
        }
        let mut gains = Vec::with_capacity(taps);
        let mut g = 1.0;
        for _ in 0..taps {
            g *= self.decay;
            gains.push(g);
        }
        w.filter(|| Taps {
            hist: vec![0.0; taps * d],
            pos: 0,
            d,
            gains: gains.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn impulse_gives_exactly_taps_repeats() {
        let e = make(&INFO, "delay=2,phase=0,decay=0.5,taps=2");
        let mut b = u8_of(&[0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert_eq!(b, u8_of(&[0.5, 0.0, 0.25, 0.0, 0.125, 0.0, 0.0, 0.0, 0.0]));
    }

    #[test]
    fn delay_longer_than_window_is_identity() {
        let e = make(&INFO, "delay=100rows");
        let orig = noise(300, 9);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::U8);
        assert_eq!(b, orig);
    }
}
