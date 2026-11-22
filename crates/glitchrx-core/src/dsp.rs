//! Audio building blocks shared by the effects: delay lines, Freeverb's comb
//! and allpass filters, and a deterministic `exp`.
//!
//! Everything here uses only `+ - * /` and comparisons on `f64` (SPEC D-25).
//! The platform's `exp`, `sin` and `pow` may differ in the last bit between
//! operating systems, which would change output bytes; IEEE basic operations
//! never do.

/// `e^x` from basic operations: split `x = n*ln2 + r` with `|r| <= ln2/2`,
/// sum the Taylor series for `e^r` (converged well below f64 precision by 20
/// terms at that size), and scale by `2^n` by building the exponent bits.
pub fn exp(x: f64) -> f64 {
    const LN2: f64 = std::f64::consts::LN_2;
    if x.is_nan() {
        return x;
    }
    let x = x.clamp(-700.0, 700.0);
    let n = (x / LN2).round();
    let r = x - n * LN2;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..=20 {
        term = term * r / k as f64;
        sum += term;
    }
    // 2^n as an exact f64: biased exponent n + 1023, zero mantissa.
    let n = n as i64;
    let two_n = f64::from_bits(((n + 1023) as u64) << 52);
    sum * two_n
}

/// Decibels to a linear gain factor: `10^(db/20)`.
pub fn db_to_gain(db: f64) -> f64 {
    exp(db * (std::f64::consts::LN_10 / 20.0))
}

/// A fixed-length delay line (ring buffer) of past values.
pub struct Ring {
    buf: Vec<f64>,
    pos: usize,
}

impl Ring {
    /// A line `len` samples long, full of silence. `len` must be >= 1.
    pub fn new(len: usize) -> Ring {
        Ring {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    /// The value written `len` samples ago.
    #[inline]
    pub fn oldest(&self) -> f64 {
        self.buf[self.pos]
    }

    /// Replace the oldest value with `v` and advance.
    #[inline]
    pub fn push(&mut self, v: f64) {
        self.buf[self.pos] = v;
        self.pos += 1;
        if self.pos == self.buf.len() {
            self.pos = 0;
        }
    }
}

/// Freeverb's lowpass-feedback comb filter.
pub struct Comb {
    ring: Ring,
    store: f64,
    feedback: f64,
    damp: f64,
}

impl Comb {
    pub fn new(len: usize, feedback: f64, damp: f64) -> Comb {
        Comb {
            ring: Ring::new(len),
            store: 0.0,
            feedback,
            damp,
        }
    }

    #[inline]
    pub fn process(&mut self, input: f64) -> f64 {
        let out = self.ring.oldest();
        // One-pole lowpass in the feedback path: high frequencies die out
        // faster, which is what "damping" means.
        self.store = out * (1.0 - self.damp) + self.store * self.damp;
        self.ring.push(input + self.store * self.feedback);
        out
    }
}

/// Freeverb's allpass diffuser (feedback 0.5).
pub struct Allpass {
    ring: Ring,
}

impl Allpass {
    pub fn new(len: usize) -> Allpass {
        Allpass {
            ring: Ring::new(len),
        }
    }

    #[inline]
    pub fn process(&mut self, input: f64) -> f64 {
        let delayed = self.ring.oldest();
        self.ring.push(input + delayed * 0.5);
        delayed - input
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exp_matches_std_closely() {
        for x in [-20.0, -3.3, -1.0, -0.1, 0.0, 0.5, 1.0, 2.5, 10.0, 40.0] {
            let rel = (exp(x) - x.exp()).abs() / x.exp();
            assert!(rel < 1e-14, "exp({x}): {} vs {}", exp(x), x.exp());
        }
        assert_eq!(exp(0.0), 1.0);
    }

    #[test]
    fn decibels() {
        assert!((db_to_gain(20.0) - 10.0).abs() < 1e-12);
        assert!((db_to_gain(-6.0) - 0.501_187_233_627_272_2).abs() < 1e-12);
    }

    #[test]
    fn ring_delays_by_its_length() {
        let mut r = Ring::new(3);
        let mut out = Vec::new();
        for v in 1..=6 {
            out.push(r.oldest());
            r.push(v as f64);
        }
        assert_eq!(out, [0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
    }
}
