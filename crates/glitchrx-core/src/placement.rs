//! Where an effect lands: region, hits and window length.
//!
//! Every step runs on one or more *windows* of the raster: a single window
//! over its whole `region` (`hits=0`), or `hits` windows of length `len` at
//! seeded places inside it.
//!
//! Windows are always worked out at full resolution, with each hit drawn
//! as a fraction of the free space, and then mapped into the raster being
//! processed (see [`Units`]). So a hit lands at the same place in the
//! picture whether the step runs on the full raster or on a screen-sized
//! preview (SPEC D-26).

use std::ops::Range;

use crate::params::{Bound, Params, Units};
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

    /// The windows for a raster measured by `units`. Every window starts on
    /// a multiple of `frame` bytes and is a whole number of frames long, so
    /// samples always line up with the start of the raster.
    ///
    /// Only stream 0 of the step seed is used here; each hit gets its own
    /// stream for its effect randomness (see [`hit_rng`]).
    pub fn windows(&self, units: &Units, frame: usize, seed: u64) -> Vec<Range<usize>> {
        let down = |v: usize| v / frame * frame;
        let full_len = units.full_len();
        let s = down(units.full_bytes(self.start).min(full_len));
        let e = down(units.full_bytes(self.end).min(full_len));
        if s >= e {
            return Vec::new();
        }
        let full: Vec<(usize, usize)> = if self.hits == 0 {
            vec![(s, e)]
        } else {
            let span = e - s;
            let len = down(units.full_bytes(self.len)).clamp(frame, span);
            let free = (span - len) as u128;
            let mut rng = Rng::new(derive(seed, 0));
            (0..self.hits)
                .map(|_| {
                    // A 64-bit fraction of the free space: (r / 2^64) * free.
                    let at = s + down(((rng.next_u64() as u128 * free) >> 64) as usize);
                    (at, at + len)
                })
                .collect()
        };
        // Map both ends into this raster; keep at least one frame.
        let raster_len = down(units.len());
        full.into_iter()
            .filter_map(|(a, b)| {
                let a = down(units.to_raster(a)).min(raster_len);
                let b = down(units.to_raster(b)).min(raster_len).max(a + frame);
                (b <= raster_len).then_some(a..b)
            })
            .collect()
    }
}

/// Random source for hit number `hit` of a step, independent of every other
/// hit (so changing `hits` never changes what the earlier hits draw).
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

    /// 100 x 10 pixels: 300-byte rows, 3000 bytes.
    fn units() -> Units {
        Units::full(100, 10)
    }

    #[test]
    fn whole_region_when_no_hits() {
        let p = pl(
            Bound::MilliPercent(10_000),
            Bound::MilliPercent(100_000),
            0,
            Bound::Bytes(1),
        );
        assert_eq!(p.windows(&units(), 1, 1), vec![300..3000]);
        // Aligned to 2-byte frames: 2999 rounds down.
        let p = pl(Bound::Bytes(1), Bound::Bytes(2999), 0, Bound::Bytes(1));
        assert_eq!(p.windows(&units(), 2, 1), vec![0..2998]);
    }

    #[test]
    fn hits_stay_inside_region_and_are_deterministic() {
        let p = pl(Bound::Bytes(100), Bound::Bytes(900), 20, Bound::Bytes(64));
        let a = p.windows(&units(), 2, 9);
        assert_eq!(a, p.windows(&units(), 2, 9));
        assert_eq!(a.len(), 20);
        for w in &a {
            assert!(w.start >= 100 && w.end <= 900 && w.len() == 64, "{w:?}");
            assert_eq!(w.start % 2, 0);
        }
    }

    #[test]
    fn hits_land_at_the_same_place_in_a_preview() {
        let p = pl(
            Bound::MilliPercent(0),
            Bound::MilliPercent(100_000),
            5,
            Bound::MilliPercent(10_000),
        );
        let full = Units::full(600, 400);
        let preview = Units {
            full_width: 600,
            full_height: 400,
            width: 150,
            height: 100,
        };
        let a = p.windows(&full, 1, 3);
        let b = p.windows(&preview, 1, 3);
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            // Same row and column, at a quarter of the size (within one).
            let (fr, fc) = (x.start / 1800, x.start % 1800 / 3);
            let (pr, pc) = (y.start / 450, y.start % 450 / 3);
            assert!(fr.div_ceil(4).abs_diff(pr) <= 1, "row {fr} vs {pr}");
            assert!(fc.div_ceil(4).abs_diff(pc) <= 1, "column {fc} vs {pc}");
            // A 10% window is 40 full rows, 10 preview rows.
            assert_eq!(y.len(), 10 * 450);
        }
    }

    #[test]
    fn oversized_window_is_clamped() {
        let p = pl(Bound::Bytes(0), Bound::Bytes(50), 2, Bound::Bytes(500));
        assert_eq!(p.windows(&units(), 1, 1), vec![0..50, 0..50]);
    }

    #[test]
    fn empty_region_gives_no_windows() {
        let p = pl(Bound::Bytes(500), Bound::Bytes(100), 3, Bound::Bytes(10));
        assert!(p.windows(&units(), 1, 1).is_empty());
    }
}
