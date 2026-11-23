//! `hsl`: hue, saturation and lightness for the whole picture, as in
//! Photoshop's Hue/Saturation (master only).
//!
//! Each pixel goes to HSL, is turned round the colour wheel, has its
//! saturation scaled and its lightness pushed towards black or white, and
//! comes back to RGB. Greys have no hue, and scaling their saturation (0)
//! keeps them grey.
//!
//! **Colour ranges** (D-54): with `range` set, only one family of colours
//! changes, as in Photoshop's Reds, Yellows, ... Each pixel's share is its
//! closeness on the colour wheel to the range's centre (full within 15
//! degrees, fading out by 45, so neighbouring ranges overlap softly)
//! times how colourful it is (full from a chroma of 1/8 of the range), so
//! greys and near-greys, whose hue means little, are left alone.

use super::adjust::{per_pixel, to_byte};
use super::{Effect, EffectInfo, Group, PlacementDefaults, Window};
use crate::error::Result;
use crate::params::{Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "hsl",
    group: Group::Adjust,
    version: 1,
    audacity: "none (an adjustment)",
    summary: "Turn the hues, and change saturation and lightness.",
    details: "`range` limits it to one family of colours (`reds`, `yellows`, ...), \
              fading softly into the neighbouring ones; greys are left alone. \
              `hue` turns every colour round the colour wheel, in degrees (120 makes red \
              green, green blue and blue red). `saturation` from -100 (grey) to 100 \
              (twice as vivid, clipping at full). `lightness` from -100 (black) to 100 \
              (white). Greys stay grey under hue and saturation.",
    params: &[
        ParamSpec {
            name: "range",
            kind: Kind::Choice(&[
                "all", "reds", "yellows", "greens", "cyans", "blues", "magentas",
            ]),
            default: "all",
            help: "which colours change: all, or one family",
        },
        ParamSpec {
            name: "hue",
            kind: Kind::Int {
                min: -180,
                max: 180,
            },
            default: "0",
            help: "turn the colours round the wheel, in degrees",
        },
        ParamSpec {
            name: "saturation",
            kind: Kind::Int {
                min: -100,
                max: 100,
            },
            default: "30",
            help: "-100 grey, 0 unchanged, 100 twice as vivid",
        },
        ParamSpec {
            name: "lightness",
            kind: Kind::Int {
                min: -100,
                max: 100,
            },
            default: "0",
            help: "-100 black, 0 unchanged, 100 white",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    exact_preview: false,
    example: "glitchrx --hsl hue=40,saturation=-50 photo.jpg",
    build,
};

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let centre = ["reds", "yellows", "greens", "cyans", "blues", "magentas"]
        .iter()
        .position(|&r| r == p.choice("range"))
        .map(|i| i as f64);
    Ok(Box::new(Hsl {
        centre,
        // Hue in sixths of a turn, the unit the conversion works in.
        turn: p.int("hue") as f64 / 60.0,
        sat: 1.0 + p.int("saturation") as f64 / 100.0,
        light: p.int("lightness") as f64 / 100.0,
    }))
}

struct Hsl {
    /// The colour range's centre hue (0..6), or none for all colours.
    centre: Option<f64>,
    turn: f64,
    sat: f64,
    light: f64,
}

/// RGB (0..1) to hue (0..6), saturation and lightness (0..1).
pub(crate) fn to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        let h = (g - b) / d;
        if h < 0.0 { h + 6.0 } else { h }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h, s.min(1.0), l)
}

/// Hue (0..6), saturation and lightness back to RGB (0..1).
pub(crate) fn from_hsl(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (r + m, g + m, b + m)
}

/// How much a pixel of hue `h` (0..6) and chroma `c` (0..1) belongs to
/// the range centred on `centre`.
fn share(centre: f64, h: f64, c: f64) -> f64 {
    let mut d = (h - centre).abs();
    if d > 3.0 {
        d = 6.0 - d;
    }
    // 15 degrees is a quarter of a sixth; 45 three quarters.
    let near = ((0.75 - d) / 0.5).clamp(0.0, 1.0);
    near * (c * 8.0).min(1.0)
}

impl Effect for Hsl {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        per_pixel(w, |px| {
            let f = |v: u8| v as f64 / 255.0;
            let (r0, g0, b0) = (f(px[0]), f(px[1]), f(px[2]));
            let (h, s, l) = to_hsl(r0, g0, b0);
            let k = match self.centre {
                Some(c) => {
                    let chroma = r0.max(g0).max(b0) - r0.min(g0).min(b0);
                    let k = share(c, h, chroma);
                    if k == 0.0 {
                        return;
                    }
                    k
                }
                None => 1.0,
            };
            let mut h = h + self.turn;
            if h < 0.0 {
                h += 6.0;
            } else if h >= 6.0 {
                h -= 6.0;
            }
            let s = (s * self.sat).min(1.0);
            let l = if self.light < 0.0 {
                l * (1.0 + self.light)
            } else {
                l + (1.0 - l) * self.light
            };
            let (r, g, b) = from_hsl(h, s, l);
            // Only part of the way for pixels at a range's soft edge (and
            // exactly the full change otherwise, with no rounding detour).
            let mix = |a: f64, b: f64| if k == 1.0 { b } else { a + (b - a) * k };
            px[0] = to_byte(mix(r0, r));
            px[1] = to_byte(mix(g0, g));
            px[2] = to_byte(mix(b0, b));
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    #[test]
    fn hsl_round_trips_every_byte_colour_closely() {
        let mut r = crate::rng::Rng::new(3);
        for _ in 0..20_000 {
            let c = [r.next_u64() as u8, r.next_u64() as u8, r.next_u64() as u8];
            let f = |v: u8| v as f64 / 255.0;
            let (h, s, l) = to_hsl(f(c[0]), f(c[1]), f(c[2]));
            let (a, b, d) = from_hsl(h, s, l);
            assert_eq!([to_byte(a), to_byte(b), to_byte(d)], c);
        }
    }

    #[test]
    fn identity_at_zero_and_greys_stay_grey() {
        let e = make(&INFO, "hue=0,saturation=0,lightness=0");
        let orig = noise(300, 9);
        let mut b = orig.clone();
        run(e.as_ref(), &mut b, 10, Encoding::U8);
        assert_eq!(b, orig);
        let e = make(&INFO, "hue=90,saturation=100");
        let mut b = vec![77, 77, 77, 200, 200, 200];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![77, 77, 77, 200, 200, 200]);
    }

    #[test]
    fn a_colour_range_changes_only_its_colours() {
        // Reds to grey: red goes, blue and grey stay.
        let e = make(&INFO, "range=reds,saturation=-100");
        let mut b = vec![220, 30, 30, 30, 30, 220, 128, 128, 128];
        run(e.as_ref(), &mut b, 3, Encoding::U8);
        assert_eq!(&b[3..], &[30, 30, 220, 128, 128, 128]);
        assert!(b[0].abs_diff(b[1]) < 2, "{b:?}");
        // Orange (30 degrees) sits between reds and yellows: half way.
        assert!((share(0.0, 0.5, 1.0) - 0.5).abs() < 1e-12);
        assert!((share(1.0, 0.5, 1.0) - 0.5).abs() < 1e-12);
        // Magenta-ish reds across 0 count too; a nearly grey red barely.
        assert_eq!(share(0.0, 5.9, 1.0), 1.0);
        assert!(share(0.0, 0.0, 0.05) < 0.5);
    }

    #[test]
    fn hue_turns_and_the_extremes() {
        let e = make(&INFO, "hue=120,saturation=0");
        let mut b = vec![255, 0, 0, 0, 255, 0];
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, vec![0, 255, 0, 0, 0, 255]);
        let e = make(&INFO, "saturation=-100");
        let mut b = vec![255, 0, 0];
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert_eq!(b, vec![128, 128, 128]);
        let e = make(&INFO, "saturation=0,lightness=100");
        let mut b = vec![10, 90, 200];
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert_eq!(b, vec![255, 255, 255]);
    }
}
