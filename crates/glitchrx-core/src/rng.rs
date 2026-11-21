//! Deterministic randomness.
//!
//! Reproducibility is a hard requirement: the same input, recipe and seed must
//! give the same output bytes on every machine and every future build. Two
//! details make that hold:
//!
//! 1. ChaCha8 is a fixed, published algorithm. Its output for a given 32-byte
//!    key never changes. We build that key ourselves from the `u64` seed with
//!    SplitMix64 rather than calling `SeedableRng::seed_from_u64`, whose
//!    expansion is a library detail that could change between major versions.
//! 2. We never use `rand`'s distribution helpers (their value streams are not
//!    promised stable). Bounded integers and shuffles are implemented here.

use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

/// Largest seed value we hand out. Seeds are stored in TOML recipes, and TOML
/// integers are signed 64-bit, so we stay within `i64::MAX`.
pub const MAX_SEED: u64 = i64::MAX as u64;

/// One step of SplitMix64: a tiny, well-studied generator that turns any
/// 64-bit state (even 0 or 1) into well-mixed output. Used only for seeding.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Derive an independent sub-seed from a parent seed and a stream number.
///
/// Used to give each chain step and each hit its own randomness, so that the
/// values one part draws never depend on how many values another part drew.
/// The result is kept within [`MAX_SEED`] so it can be written to a recipe.
pub fn derive(seed: u64, stream: u64) -> u64 {
    let mut s = seed ^ splitmix64(&mut stream.wrapping_mul(0xD1B5_4A32_D192_ED03));
    splitmix64(&mut s) & MAX_SEED
}

/// Seeded random source with the few sampling helpers the effects need.
pub struct Rng(ChaCha8Rng);

impl Rng {
    /// Create a generator from a 64-bit seed (see module docs for why the key
    /// is expanded by hand).
    pub fn new(seed: u64) -> Self {
        let mut state = seed;
        let mut key = [0u8; 32];
        for chunk in key.chunks_exact_mut(8) {
            chunk.copy_from_slice(&splitmix64(&mut state).to_le_bytes());
        }
        Rng(ChaCha8Rng::from_seed(key))
    }

    /// Next raw 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    /// Uniform integer in `0..n`. `n` must be non-zero.
    ///
    /// Plain `x % n` would favour small values whenever `n` does not divide
    /// 2^64. Rejection sampling discards the uneven tail so every value is
    /// equally likely.
    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "Rng::below called with n = 0");
        // Largest multiple of n that fits in u64; values at or above it are
        // rejected. `u64::MAX - (u64::MAX % n)` would overflow the reasoning
        // when n is a power of two, so compute the rejection zone this way.
        let zone = u64::MAX - (u64::MAX - n + 1) % n;
        loop {
            let x = self.next_u64();
            if x <= zone {
                return x % n;
            }
        }
    }

    /// Fisher-Yates shuffle: every permutation equally likely.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_stream() {
        let a: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..8).map(|_| r.next_u64()).collect()
        };
        let b: Vec<u64> = {
            let mut r = Rng::new(42);
            (0..8).map(|_| r.next_u64()).collect()
        };
        assert_eq!(a, b);
    }

    #[test]
    fn stream_is_pinned() {
        // Golden value: if this changes, every recipe ever saved would replay
        // differently. Do not update it without bumping every effect VERSION.
        let mut r = Rng::new(0);
        assert_eq!(r.next_u64(), PINNED_FIRST_U64);
    }

    // Recorded from the first run of the test suite and frozen.
    const PINNED_FIRST_U64: u64 = 13_804_888_775_535_289_832;

    #[test]
    fn below_stays_in_range() {
        let mut r = Rng::new(7);
        for n in [1u64, 2, 3, 7, 255, 256, 1 << 40] {
            for _ in 0..200 {
                assert!(r.below(n) < n);
            }
        }
    }

    #[test]
    fn derive_fits_in_toml_integer() {
        for s in 0..1000 {
            assert!(derive(u64::MAX - s, s) <= MAX_SEED);
        }
    }

    #[test]
    fn shuffle_is_permutation() {
        let mut r = Rng::new(3);
        let mut v: Vec<u32> = (0..100).collect();
        r.shuffle(&mut v);
        let mut sorted = v.clone();
        sorted.sort();
        assert_eq!(sorted, (0..100).collect::<Vec<_>>());
        assert_ne!(v, sorted);
    }
}
