//! `echo`: Audacity's Echo, a feedback delay.

use super::{Effect, EffectInfo, Filter, PHASE, PlacementDefaults, Window};
use crate::dsp::Ring;
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "echo",
    version: 1,
    audacity: "Echo",
    summary: "Feedback delay: every sample adds a fading copy of the output `delay` earlier.",
    details: "`y[n] = x[n] + decay * y[n - delay]`. A delay of a fraction of a row ghosts the \
              picture sideways, whole rows ghost it downwards. Repeats stack until they fade, \
              and the sum is clipped, which is where the bright bands come from.",
    params: &[
        ParamSpec {
            name: "delay",
            kind: Kind::Size,
            default: "0.33rows",
            help: "distance to the repeat (rows, px, % or bytes)",
        },
        PHASE,
        ParamSpec {
            name: "decay",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.5",
            help: "loudness of each repeat relative to the one before",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    example: "glitchrx --echo delay=0.33rows,decay=0.5 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Echo {
        delay: p.size("delay"),
        phase: p.int("phase") as usize,
        decay: p.float("decay"),
    }))
}

struct Echo {
    delay: Bound,
    phase: usize,
    decay: f64,
}

/// `y[n] = x[n] + decay * y[n - d]`, with the past outputs in a ring.
struct Feedback {
    ring: Ring,
    decay: f64,
}

impl Filter for Feedback {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            let y = *v + self.decay * self.ring.oldest();
            self.ring.push(y);
            *v = y;
        }
    }
}

impl Effect for Echo {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let d = w.samples(w.units.offset(self.delay, self.phase));
        // A delay longer than the window can never be heard; a ring as long
        // as the window behaves the same (it never wraps) and caps memory.
        let len = d.min(w.frames().max(1));
        let decay = self.decay;
        w.filter(|| Feedback {
            ring: Ring::new(len),
            decay,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn impulse_repeats_and_fades() {
        // One pixel-wide rows of 1 px = 3 bytes; delay 2 bytes, no phase.
        let e = make(&INFO, "delay=2,phase=0,decay=0.5");
        let mut b = u8_of(&[0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert_eq!(
            b,
            u8_of(&[0.5, 0.0, 0.25, 0.0, 0.125, 0.0, 0.0625, 0.0, 0.03125])
        );
    }

    #[test]
    fn feedback_clips_at_the_top() {
        let e = make(&INFO, "delay=1,phase=0,decay=1");
        let mut b = u8_of(&[0.5; 6]);
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![192, 255, 255, 255, 255, 255]);
    }

    #[test]
    fn zero_decay_is_identity() {
        let e = make(&INFO, "decay=0");
        let orig = noise(300, 4);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::S16Le);
        assert_eq!(b, orig);
    }
}
