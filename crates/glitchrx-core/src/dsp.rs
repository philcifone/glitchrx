//! Audio building blocks shared by the effects: delay lines, Freeverb's comb
//! and allpass filters, a slow sweep oscillator (LFO), and deterministic
//! `exp`, `ln`, `pow` and sine.
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

/// Natural logarithm from basic operations, for `x > 0` (0 and below give
/// negative infinity). Split `x = m * 2^e` with `m` in [1, 2) by reading
/// the float's bits, then `ln m = 2 atanh(z)` with `z = (m - 1) / (m + 1)`,
/// at most 1/3, whose odd series `z + z^3/3 + z^5/5 + ...` is below f64
/// precision after 30 terms.
pub fn ln(x: f64) -> f64 {
    const LN2: f64 = std::f64::consts::LN_2;
    if x.is_nan() {
        return x;
    }
    if x <= 0.0 {
        return f64::NEG_INFINITY;
    }
    if x.is_infinite() {
        return x;
    }
    // Subnormals: scale into the normal range first (exactly, by 2^54).
    let (x, extra) = if x < f64::MIN_POSITIVE {
        (x * 18014398509481984.0, -54i64)
    } else {
        (x, 0)
    };
    let bits = x.to_bits();
    let e = ((bits >> 52) & 0x7ff) as i64 - 1023 + extra;
    let m = f64::from_bits((bits & 0x000f_ffff_ffff_ffff) | (1023u64 << 52));
    let z = (m - 1.0) / (m + 1.0);
    let z2 = z * z;
    let mut term = z;
    let mut sum = 0.0;
    for k in 0..30 {
        sum += term / (2 * k + 1) as f64;
        term *= z2;
    }
    e as f64 * LN2 + 2.0 * sum
}

/// `x^y` for `x >= 0`, as `exp(y ln x)`; `0^y` is 0 for `y > 0` and 1 for
/// `y = 0`.
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 {
        return 1.0;
    }
    // Exact where it is easy to be: exp(ln x) is a hair off x.
    if y == 1.0 {
        return x;
    }
    if x <= 0.0 {
        return 0.0;
    }
    exp(y * ln(x))
}

/// `sin(2 pi t)`, with `t` in turns (1.0 is a full circle), from basic
/// operations only.
///
/// Working in turns makes range reduction exact: `t - round(t)` loses
/// nothing, where reducing radians by a rounded 2 pi would. The symmetry
/// `sin(x) = sin(pi - x)` then folds the angle into a quarter circle,
/// where the Taylor series to x^19 is accurate to about 1e-16.
pub fn sin_turns(t: f64) -> f64 {
    const TAU: f64 = std::f64::consts::TAU;
    let mut r = t - t.round(); // -0.5 ..= 0.5
    if r > 0.25 {
        r = 0.5 - r;
    } else if r < -0.25 {
        r = -0.5 - r;
    }
    let x = r * TAU; // -pi/2 ..= pi/2
    let x2 = x * x;
    // Horner form of x - x^3/3! + x^5/5! - ... - x^19/19!, innermost first.
    let mut sum = 0.0;
    for k in (1..=9).rev() {
        let n = (2 * k + 1) as f64;
        sum = (1.0 - sum) * x2 / (n * (n - 1.0));
    }
    x * (1.0 - sum)
}

/// `cos(2 pi t)`, with `t` in turns.
pub fn cos_turns(t: f64) -> f64 {
    sin_turns(t + 0.25)
}

/// How often (in samples) an [`Lfo`] yields a new value. Effects recompute
/// their swept coefficients only then, which is what makes them fast: the
/// exponentials and sines cost far more than the filtering. Audacity does
/// the same (every 20 samples in its phaser, 30 in its wahwah); a sweep
/// lasts thousands of samples, so the steps never show.
pub const LFO_EVERY: u64 = 16;

/// A low-frequency oscillator, as Audacity's phaser and wahwah use, with
/// its period in samples rather than Hz. Yields `(1 + cos)/2`, a smooth
/// sweep between 0 and 1, every [`LFO_EVERY`] samples.
pub struct Lfo {
    period: f64,
    start: f64,
    n: u64,
}

impl Lfo {
    /// `period` in samples (at least 1); `start` in turns.
    pub fn new(period: usize, start: f64) -> Lfo {
        Lfo {
            period: period.max(1) as f64,
            start,
            n: 0,
        }
    }

    /// Advance one sample. Every [`LFO_EVERY`] samples (starting with the
    /// first) returns the sweep's value there, 0 ..= 1; otherwise `None`,
    /// and the caller keeps using the last value.
    pub fn tick(&mut self) -> Option<f64> {
        let n = self.n;
        self.n += 1;
        if !n.is_multiple_of(LFO_EVERY) {
            return None;
        }
        // Phase from the sample count each time, not by adding up a step,
        // so rounding never drifts over millions of samples.
        let t = self.start + (n % self.period as u64) as f64 / self.period;
        Some((1.0 + cos_turns(t)) * 0.5)
    }
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
    fn ln_and_pow_match_the_platform_closely() {
        for &x in &[
            1e-300, 1e-9, 0.001, 0.1, 0.5, 1.0, 1.5, 2.0, 3.0, 10.0, 255.0, 1e9, 1e300,
        ] {
            let (ours, std) = (ln(x), x.ln());
            assert!(
                (ours - std).abs() <= 1e-14 * std.abs().max(1.0),
                "ln({x}): {ours} vs {std}"
            );
        }
        assert_eq!(ln(1.0), 0.0);
        assert_eq!(ln(0.0), f64::NEG_INFINITY);
        for &(x, y) in &[
            (0.5, 2.2),
            (0.25, 0.5),
            (0.9, 1.0 / 2.2),
            (2.0, 10.0),
            (0.01, 3.0),
        ] {
            let (ours, std) = (pow(x, y), x.powf(y));
            assert!(
                (ours - std).abs() <= 1e-13 * std.max(1e-300),
                "pow({x}, {y}): {ours} vs {std}"
            );
        }
        assert_eq!(pow(0.0, 2.0), 0.0);
        assert_eq!(pow(0.0, 0.0), 1.0);
    }

    #[test]
    fn exp_matches_std_closely() {
        for x in [-20.0, -3.3, -1.0, -0.1, 0.0, 0.5, 1.0, 2.5, 10.0, 40.0] {
            let rel = (exp(x) - x.exp()).abs() / x.exp();
            assert!(rel < 1e-14, "exp({x}): {} vs {}", exp(x), x.exp());
        }
        assert_eq!(exp(0.0), 1.0);
    }

    #[test]
    fn sine_matches_std_closely() {
        for i in -400..=400 {
            let t = i as f64 / 97.0;
            let want = (t * std::f64::consts::TAU).sin();
            assert!((sin_turns(t) - want).abs() < 1e-14, "sin_turns({t})");
            let want = (t * std::f64::consts::TAU).cos();
            assert!((cos_turns(t) - want).abs() < 1e-14, "cos_turns({t})");
        }
        assert_eq!(sin_turns(0.0), 0.0);
        assert!((sin_turns(0.25) - 1.0).abs() < 1e-15);
        assert!((cos_turns(0.0) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn lfo_sweeps_between_zero_and_one() {
        let p = 8 * LFO_EVERY as usize;
        let mut lfo = Lfo::new(p, 0.0);
        let all: Vec<Option<f64>> = (0..=p).map(|_| lfo.tick()).collect();
        // A value every LFO_EVERY samples, nothing in between.
        assert!(
            all.iter()
                .enumerate()
                .all(|(i, v)| v.is_some() == (i as u64).is_multiple_of(LFO_EVERY))
        );
        let v: Vec<f64> = all.into_iter().flatten().collect();
        assert!((v[0] - 1.0).abs() < 1e-15); // starts at the top
        assert!(v[4].abs() < 1e-15); // bottom half a period later
        assert_eq!(v[8], v[0]); // and repeats
        assert!(v.iter().all(|x| (0.0..=1.0).contains(x)));
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
