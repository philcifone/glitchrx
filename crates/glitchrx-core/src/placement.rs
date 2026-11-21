//! Where an effect lands: region, hits and window length.
//!
//! Glitches look best sparse and localized. Every byte effect therefore runs
//! on a handful of windows ("hits") placed at seeded random offsets inside a
//! region of the payload, rather than on the whole buffer.

use std::ops::Range;

use crate::params::{Bound, Params};
use crate::rng::{Rng, derive};

/// Resolved placement settings for one effect step.
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub start: Bound,
    pub end: Bound,
    /// Number of windows; 0 means one window spanning the whole region.
    pub hits: u32,
    pub len: Bound,
}

impl Placement {
    /// Read the shared `region`, `hits` and `len` parameters.
    pub fn from_params(p: &Params) -> Placement {
        let (start, end) = p.region("region");
        Placement {
            start,
            end,
            hits: p.int("hits") as u32,
            len: p.size("len"),
        }
    }

    /// Compute the windows for a payload of `payload_len` bytes.
    ///
    /// Only stream 0 of the step seed is used here; each hit gets its own
    /// stream for its effect randomness (see [`hit_rng`]).
    pub fn windows(&self, payload_len: usize, seed: u64) -> Vec<Range<usize>> {
        let s = self.start.resolve(payload_len);
        let e = self.end.resolve(payload_len);
        if s >= e {
            return Vec::new();
        }
        if self.hits == 0 {
            return vec![s..e];
        }
        let span = e - s;
        let len = self.len.resolve(payload_len).clamp(1, span);
        let mut rng = Rng::new(derive(seed, 0));
        (0..self.hits)
            .map(|_| {
                let at = s + rng.below((span - len + 1) as u64) as usize;
                at..at + len
            })
            .collect()
    }
}

/// Random source for hit number `hit` of a step. Independent of every other
/// hit, so hits can be inverted in reverse order and still draw the same
/// values they drew going forward.
pub fn hit_rng(seed: u64, hit: usize) -> Rng {
    Rng::new(derive(seed, hit as u64 + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pl(start: Bound, end: Bound, hits: u32, len: Bound) -> Placement {
        Placement {
            start,
            end,
            hits,
            len,
        }
    }

    #[test]
    fn whole_region_when_no_hits() {
        let p = pl(
            Bound::MilliPercent(10_000),
            Bound::MilliPercent(100_000),
            0,
            Bound::Bytes(1),
        );
        assert_eq!(p.windows(1000, 1), vec![100..1000]);
    }

    #[test]
    fn hits_stay_inside_region_and_are_deterministic() {
        let p = pl(Bound::Bytes(100), Bound::Bytes(900), 20, Bound::Bytes(64));
        let a = p.windows(1000, 9);
        assert_eq!(a, p.windows(1000, 9));
        assert_eq!(a.len(), 20);
        for w in &a {
            assert!(w.start >= 100 && w.end <= 900 && w.len() == 64);
        }
    }

    #[test]
    fn oversized_window_is_clamped() {
        let p = pl(Bound::Bytes(0), Bound::Bytes(50), 2, Bound::Bytes(500));
        assert_eq!(p.windows(1000, 1), vec![0..50, 0..50]);
    }

    #[test]
    fn empty_region_gives_no_windows() {
        let p = pl(Bound::Bytes(500), Bound::Bytes(100), 3, Bound::Bytes(10));
        assert!(p.windows(1000, 1).is_empty());
    }
}
