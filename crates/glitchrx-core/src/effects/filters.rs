//! The filters (SPEC 6.7, D-71): `lowpass`, `highpass`, `bandpass`,
//! `notch` and `basstreble`, after Audacity's Low-Pass Filter, High-Pass
//! Filter, Notch Filter and Bass and Treble (band-pass is what its Filter
//! Curve EQ makes of a bump).
//!
//! All five are biquads, the two-pole, two-zero section audio filters are
//! built from, designed with Robert Bristow-Johnson's Audio EQ Cookbook.
//! Steeper low- and high-pass filters chain several, with the Q of each
//! placed as a Butterworth filter's poles are, so together they stay flat
//! up to the cutoff. Everything runs in f64 with the trigonometry from
//! `dsp.rs`, so the result is the same on every machine (D-25).
//!
//! The cutoff is a distance (`size`), not a frequency: a picture has no
//! sample rate, while "detail finer than 8 px" means something on a
//! photo. A size of P samples is a cutoff of 1/P cycles per sample. The
//! filters run one way, forward through the data as Audacity's do, so
//! their delay smears to the right and on down the rows.

use super::{Effect, EffectInfo, Filter, Group, PlacementDefaults, Window};
use crate::dsp::{cos_turns, db_to_gain, sin_turns};
use crate::error::Result;
use crate::params::{Bound, Kind, ParamSpec};
use crate::rng::Rng;

// ---------------------------------------------------------------------------
// The biquad and its designs

/// One biquad section in direct form I: coefficients divided by `a0`, and
/// the last two inputs and outputs.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    /// A section from the cookbook's six coefficients.
    fn new(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Biquad {
        Biquad {
            b: [b0 / a0, b1 / a0, b2 / a0],
            a: [a1 / a0, a2 / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    /// A first-order section (one pole, one zero; `b2 = a2 = 0`), for the
    /// gentlest rolloff, 6 dB per octave.
    fn first_order(b0: f64, b1: f64, a1: f64) -> Biquad {
        Biquad::new(b0, b1, 0.0, 1.0, a1, 0.0)
    }

    fn tick(&mut self, input: f64) -> f64 {
        let ([b0, b1, b2], [a1, a2]) = (self.b, self.a);
        let out = b0 * input + b1 * self.x[0] + b2 * self.x[1] - a1 * self.y[0] - a2 * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [out, self.y[0]];
        out
    }
}

/// The cookbook's shared terms for a cutoff of `f` cycles per sample:
/// cos and sin of the angle `w0 = 2 pi f`.
fn angle(f: f64) -> (f64, f64) {
    (cos_turns(f), sin_turns(f))
}

/// A Butterworth low- or high-pass of `order` (1, 2, 4, 6 or 8 poles) at
/// `f` cycles per sample, as a chain of sections.
fn butterworth(high: bool, order: usize, f: f64) -> Vec<Biquad> {
    if order == 1 {
        // The bilinear one-pole: K = tan(pi f).
        let (cs, sn) = angle(f / 2.0);
        let k = sn / cs;
        let a1 = (k - 1.0) / (k + 1.0);
        let b0 = if high { 1.0 } else { k } / (1.0 + k);
        let b1 = if high { -b0 } else { b0 };
        return vec![Biquad::first_order(b0, b1, a1)];
    }
    let (cs, sn) = angle(f);
    (1..=order / 2)
        .map(|i| {
            // Stage i of N/2: Q = 1 / (2 cos((2i - 1) pi / 2N)); in turns
            // the angle is (2i - 1) / 4N.
            let q = 1.0 / (2.0 * cos_turns((2 * i - 1) as f64 / (4 * order) as f64));
            let alpha = sn / (2.0 * q);
            let (a0, a1, a2) = (1.0 + alpha, -2.0 * cs, 1.0 - alpha);
            if high {
                let k = (1.0 + cs) / 2.0;
                Biquad::new(k, -2.0 * k, k, a0, a1, a2)
            } else {
                let k = (1.0 - cs) / 2.0;
                Biquad::new(k, 2.0 * k, k, a0, a1, a2)
            }
        })
        .collect()
}

/// The cookbook band-pass (a peak of 0 dB) or notch at `f`, `q` wide.
fn band(notch: bool, f: f64, q: f64) -> Biquad {
    let (cs, sn) = angle(f);
    let alpha = sn / (2.0 * q);
    let (a0, a1, a2) = (1.0 + alpha, -2.0 * cs, 1.0 - alpha);
    if notch {
        Biquad::new(1.0, -2.0 * cs, 1.0, a0, a1, a2)
    } else {
        Biquad::new(alpha, 0.0, -alpha, a0, a1, a2)
    }
}

/// The cookbook low or high shelf at `f` with a slope of 1, raising (or
/// lowering) one side by `db`.
fn shelf(high: bool, f: f64, db: f64) -> Biquad {
    // A is the square root of the gain: 10^(dB / 40).
    let a = db_to_gain(db / 2.0);
    let (cs, sn) = angle(f);
    // With S = 1, alpha = sin(w0) / 2 * sqrt(2); 2 sqrt(A) alpha is then
    // sin(w0) sqrt(2A). IEEE rounds sqrt exactly, so it is deterministic.
    let r = sn * (2.0 * a).sqrt();
    let (p, m) = (a + 1.0, a - 1.0);
    if high {
        Biquad::new(
            a * (p + m * cs + r),
            -2.0 * a * (m + p * cs),
            a * (p + m * cs - r),
            p - m * cs + r,
            2.0 * (m - p * cs),
            p - m * cs - r,
        )
    } else {
        Biquad::new(
            a * (p - m * cs + r),
            2.0 * a * (m - p * cs),
            a * (p - m * cs - r),
            p + m * cs + r,
            -2.0 * (m + p * cs),
            p + m * cs - r,
        )
    }
}

/// A chain of sections, then a gain; one per window and channel, so it
/// always starts at rest.
struct Chain {
    stages: Vec<Biquad>,
    gain: f64,
}

impl Filter for Chain {
    fn process(&mut self, x: &mut [f64]) {
        for v in x {
            let mut s = *v;
            for b in &mut self.stages {
                s = b.tick(s);
            }
            *v = s * self.gain;
        }
    }
}

/// A distance as cycles per sample for this window: one cycle per `size`.
/// Sizes under 2 samples are held at 2, the highest frequency samples can
/// carry (half the sample rate); a preview may hit that where the full
/// picture does not, which is why these effects render exactly on pause.
fn cycles(w: &Window<'_>, size: Bound) -> f64 {
    let bytes = w.units.to_raster(w.units.full_bytes(size));
    let samples = bytes as f64 / w.frame() as f64;
    1.0 / samples.max(2.0)
}

// ---------------------------------------------------------------------------
// The effects

const PLACEMENT: PlacementDefaults = PlacementDefaults {
    region: "0%:100%",
    hits: "0",
    len: "10%",
};

/// Audacity's rolloff choices, in dB per octave.
const ROLLOFF: ParamSpec = ParamSpec {
    name: "rolloff",
    kind: Kind::Choice(&["6", "12", "24", "36", "48"]),
    default: "12",
    help: "how sharply it cuts past the size, in dB per octave; steep ones ring",
};

const Q: ParamSpec = ParamSpec {
    name: "q",
    kind: Kind::Float {
        min: 0.3,
        max: 20.0,
    },
    default: "1",
    help: "how narrow the band is; high values ring",
};

/// Poles for a rolloff: 6 dB per octave for each.
fn order(rolloff: &str) -> usize {
    rolloff.parse::<usize>().unwrap_or(12) / 6
}

pub static LOWPASS: EffectInfo = EffectInfo {
    id: "lowpass",
    group: Group::Filter,
    version: 1,
    upgrades: &[],
    audacity: "Low-Pass Filter",
    summary: "Removes detail finer than a size: smears along the rows; in Mono, colors drain to grey.",
    details: "A Butterworth low-pass: detail finer than `size` fades out, broad shapes pass. \
              It runs forward through the data, so it smears to the right and on into the \
              next row; larger sizes smear further. Steep rolloffs ring into faint repeated \
              edges. In Mono, R, G and B sit side by side in one stream, the finest detail \
              there is, so the filter blends them and colors drain to grey; with RGB \
              channels each color is smeared on its own and keeps its hue.",
    params: &[
        ParamSpec {
            name: "size",
            kind: Kind::Size,
            default: "16px",
            help: "the finest detail kept (rows, px, % or bytes)",
        },
        ROLLOFF,
    ],
    placement: PLACEMENT,
    random_within: false,
    exact_preview: true,
    strokes: false,
    example: "glitchrx --lowpass size=40px,rolloff=24 photo.jpg",
    build: |p| {
        Ok(Box::new(Pass {
            high: false,
            size: p.size("size"),
            order: order(p.choice("rolloff")),
        }))
    },
};

pub static HIGHPASS: EffectInfo = EffectInfo {
    id: "highpass",
    group: Group::Filter,
    version: 1,
    upgrades: &[],
    audacity: "High-Pass Filter",
    summary: "Keeps only detail finer than a size: brightness flattens, edges stand out; RGB gives a grey emboss.",
    details: "A Butterworth high-pass: broad shapes lose their brightness and only detail finer \
              than `size` stays, so edges stand out and lean to the right. In Mono, R, G and B \
              alternate in the data, which is finer than any size, so color survives while \
              brightness flattens: a washed, lit-from-the-side look. With RGB channels each \
              color is filtered alone and flat areas fall to the middle value (grey in u8): \
              the classic grey emboss. Larger sizes keep more of the picture; steep \
              rolloffs ring.",
    params: &[
        ParamSpec {
            name: "size",
            kind: Kind::Size,
            default: "8px",
            help: "the broadest detail kept (rows, px, % or bytes)",
        },
        ROLLOFF,
    ],
    placement: PLACEMENT,
    random_within: false,
    exact_preview: true,
    strokes: false,
    example: "glitchrx --highpass size=12px photo.jpg",
    build: |p| {
        Ok(Box::new(Pass {
            high: true,
            size: p.size("size"),
            order: order(p.choice("rolloff")),
        }))
    },
};

pub static BANDPASS: EffectInfo = EffectInfo {
    id: "bandpass",
    group: Group::Filter,
    version: 1,
    upgrades: &[],
    audacity: "Filter Curve EQ (a band)",
    summary: "Keeps one scale of detail around a size: one texture picked out on grey.",
    details: "A band-pass biquad (the cookbook's, peaking at 0 dB): detail around `size` \
              passes, finer and broader detail fade. Like high-pass, flat areas sink to the \
              middle value, but softer, with one texture picked out. High `q` narrows the \
              band until it rings into ripples. In Mono, colors fade towards a muted tint.",
    params: &[
        ParamSpec {
            name: "size",
            kind: Kind::Size,
            default: "6px",
            help: "the scale of detail kept (rows, px, % or bytes)",
        },
        Q,
    ],
    placement: PLACEMENT,
    random_within: false,
    exact_preview: true,
    strokes: false,
    example: "glitchrx --bandpass size=10px,q=4 photo.jpg",
    build: |p| {
        Ok(Box::new(Band {
            notch: false,
            size: p.size("size"),
            q: p.float("q"),
        }))
    },
};

pub static NOTCH: EffectInfo = EffectInfo {
    id: "notch",
    group: Group::Filter,
    version: 1,
    upgrades: &[],
    audacity: "Notch Filter",
    summary: "Removes one scale of detail around a size: reshapes patterns and moire.",
    details: "A notch biquad (the cookbook's): detail around `size` is removed and the rest \
              passes. Subtle on most photos; on regular patterns (fabric, screens, halftones) \
              it removes or reshapes the moire, and on a size near a pattern's own it leaves \
              ghostly beats.",
    params: &[
        ParamSpec {
            name: "size",
            kind: Kind::Size,
            default: "6px",
            help: "the scale of detail removed (rows, px, % or bytes)",
        },
        Q,
    ],
    placement: PLACEMENT,
    random_within: false,
    exact_preview: true,
    strokes: false,
    example: "glitchrx --notch size=3px,q=2 photo.jpg",
    build: |p| {
        Ok(Box::new(Band {
            notch: true,
            size: p.size("size"),
            q: p.float("q"),
        }))
    },
};

const DB: Kind = Kind::Float {
    min: -30.0,
    max: 30.0,
};

pub static BASSTREBLE: EffectInfo = EffectInfo {
    id: "basstreble",
    group: Group::Filter,
    version: 1,
    upgrades: &[],
    audacity: "Bass and Treble",
    summary: "Boosts or cuts broad shapes (bass) and fine detail (treble): waves of color, sharp fringes.",
    details: "Two shelving biquads, as Audacity's Bass and Treble: `bass` raises or lowers \
              detail broader than `low`, `treble` detail finer than `high`, then `gain` sets \
              the level. Results clip at the limits, as amplify's do. Bass boosts give soft \
              horizontal waves of brightness that blow out to white and black when pushed. \
              Treble sharpens and fringes edges; in Mono it also boosts the alternation of R, \
              G and B, so colors grow vivid and neon.",
    params: &[
        ParamSpec {
            name: "bass",
            kind: DB,
            default: "6",
            help: "dB for broad shapes, below low",
        },
        ParamSpec {
            name: "treble",
            kind: DB,
            default: "0",
            help: "dB for fine detail, above high",
        },
        ParamSpec {
            name: "gain",
            kind: DB,
            default: "0",
            help: "dB for everything, after the shelves",
        },
        ParamSpec {
            name: "low",
            kind: Kind::Size,
            default: "32px",
            help: "where bass begins: detail broader than this (rows, px, % or bytes)",
        },
        ParamSpec {
            name: "high",
            kind: Kind::Size,
            default: "3px",
            help: "where treble begins: detail finer than this (rows, px, % or bytes)",
        },
    ],
    placement: PLACEMENT,
    random_within: false,
    exact_preview: true,
    strokes: false,
    example: "glitchrx --basstreble bass=12,treble=-6 photo.jpg",
    build: |p| {
        Ok(Box::new(BassTreble {
            bass: p.float("bass"),
            treble: p.float("treble"),
            gain: p.float("gain"),
            low: p.size("low"),
            high: p.size("high"),
        }))
    },
};

struct Pass {
    high: bool,
    size: Bound,
    order: usize,
}

impl Effect for Pass {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let stages = butterworth(self.high, self.order, cycles(w, self.size));
        w.filter(|| Chain {
            stages: stages.clone(),
            gain: 1.0,
        })
    }
}

struct Band {
    notch: bool,
    size: Bound,
    q: f64,
}

impl Effect for Band {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let stage = band(self.notch, cycles(w, self.size), self.q);
        w.filter(|| Chain {
            stages: vec![stage],
            gain: 1.0,
        })
    }
}

struct BassTreble {
    bass: f64,
    treble: f64,
    gain: f64,
    low: Bound,
    high: Bound,
}

impl Effect for BassTreble {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let mut stages = Vec::new();
        // A shelf of 0 dB is the identity; skip it rather than run it.
        if self.bass != 0.0 {
            stages.push(shelf(false, cycles(w, self.low), self.bass));
        }
        if self.treble != 0.0 {
            stages.push(shelf(true, cycles(w, self.high), self.treble));
        }
        let gain = db_to_gain(self.gain);
        w.filter(|| Chain {
            stages: stages.clone(),
            gain,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    /// The gain of a chain at `f` cycles per sample, measured: run a long
    /// sine through it and, once it has settled, correlate the output with
    /// a sine and a cosine of the same frequency. (The highest sample
    /// would miss the peak at fast frequencies, where samples fall beside it.)
    fn measured(stages: Vec<Biquad>, f: f64) -> f64 {
        let mut c = Chain { stages, gain: 1.0 };
        let n = 16000;
        let mut x: Vec<f64> = (0..n).map(|i| sin_turns(f * i as f64)).collect();
        c.process(&mut x);
        let (mut s, mut k) = (0.0, 0.0);
        for (i, &v) in x.iter().enumerate().skip(n / 2) {
            s += v * sin_turns(f * i as f64);
            k += v * cos_turns(f * i as f64);
        }
        2.0 * (s * s + k * k).sqrt() / (n / 2) as f64
    }

    #[test]
    fn pass_filters_keep_one_side_of_the_cutoff() {
        for order in [1, 2, 4, 6, 8] {
            let f = 1.0 / 16.0;
            // Low-pass: a slow wave passes, a fast one is cut.
            let slow = measured(butterworth(false, order, f), f / 8.0);
            let fast = measured(butterworth(false, order, f), f * 4.0);
            assert!(slow > 0.95 && fast < 0.3, "low {order}: {slow} {fast}");
            // At the cutoff a Butterworth is -3 dB, whatever its order.
            let at = measured(butterworth(false, order, f), f);
            assert!(
                (at - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.03,
                "low {order} at cutoff: {at}"
            );
            // High-pass: the reverse.
            let slow = measured(butterworth(true, order, f), f / 8.0);
            let fast = measured(butterworth(true, order, f), f * 4.0);
            assert!(slow < 0.3 && fast > 0.95, "high {order}: {slow} {fast}");
        }
        // Steeper is steeper: an octave past the cutoff, 48 dB per octave
        // leaves far less than 12.
        let f = 1.0 / 16.0;
        let gentle = measured(butterworth(false, 2, f), f * 2.0);
        let steep = measured(butterworth(false, 8, f), f * 2.0);
        assert!(steep < gentle / 10.0, "{gentle} {steep}");
    }

    #[test]
    fn band_and_notch_pick_out_their_size() {
        let f = 1.0 / 12.0;
        assert!(measured(vec![band(false, f, 2.0)], f) > 0.95);
        assert!(measured(vec![band(false, f, 2.0)], f / 6.0) < 0.3);
        assert!(measured(vec![band(true, f, 2.0)], f) < 0.05);
        assert!(measured(vec![band(true, f, 2.0)], f / 6.0) > 0.9);
    }

    #[test]
    fn shelves_raise_their_side_only() {
        let f = 1.0 / 32.0;
        let six = db_to_gain(6.0);
        // Low shelf +6 dB: slow waves gain 6 dB, fast ones stay.
        let slow = measured(vec![shelf(false, f, 6.0)], f / 16.0);
        let fast = measured(vec![shelf(false, f, 6.0)], 0.45);
        assert!(
            (slow - six).abs() < 0.05 && (fast - 1.0).abs() < 0.05,
            "{slow} {fast}"
        );
        // High shelf -6 dB: fast waves lose 6 dB, slow ones stay.
        let slow = measured(vec![shelf(true, f, -6.0)], f / 16.0);
        let fast = measured(vec![shelf(true, f, -6.0)], 0.45);
        assert!(
            (slow - 1.0).abs() < 0.05 && (fast - 1.0 / six).abs() < 0.05,
            "{slow} {fast}"
        );
    }

    #[test]
    fn high_pass_sinks_flat_grey_to_the_middle() {
        // A flat area: after the filter settles, every byte is the middle
        // value (128 in u8), the grey the high-pass look is made of.
        let e = make(&HIGHPASS, "");
        let mut b = u8_of(&[0.6; 4000]);
        run(e.as_ref(), &mut b, 100, Encoding::U8);
        assert!(
            b[3000..].iter().all(|&v| v.abs_diff(128) <= 1),
            "{:?}",
            &b[3000..3010]
        );
    }

    #[test]
    fn low_pass_keeps_a_flat_area_and_smooths_noise() {
        let e = make(&LOWPASS, "");
        let mut b = u8_of(&[0.25; 3000]);
        run(e.as_ref(), &mut b, 50, Encoding::U8);
        let want = u8_of(&[0.25])[0];
        assert!(b[2000..].iter().all(|&v| v.abs_diff(want) <= 1));
        let orig = noise(3000, 8);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 50, Encoding::U8);
        let roughness =
            |v: &[u8]| -> u64 { v.windows(2).map(|p| p[0].abs_diff(p[1]) as u64).sum() };
        assert!(roughness(&b) < roughness(&orig) / 2);
    }

    #[test]
    fn bass_and_treble_at_zero_change_nothing() {
        let e = make(&BASSTREBLE, "bass=0");
        let orig = noise(3000, 3);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 50, Encoding::U8);
        assert_eq!(b, orig);
    }
}
