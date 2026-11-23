//! `hsl`: hue, saturation and lightness for the whole picture, as in
//! Photoshop's Hue/Saturation (master only).
//!
//! Each pixel goes to HSL, is turned round the colour wheel, has its
//! saturation scaled and its lightness pushed towards black or white, and
//! comes back to RGB. Greys have no hue, and scaling their saturation (0)
//! keeps them grey.

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
    details: "`hue` turns every colour round the colour wheel, in degrees (120 makes red \
              green, green blue and blue red). `saturation` from -100 (grey) to 100 \
              (twice as vivid, clipping at full). `lightness` from -100 (black) to 100 \
              (white). Greys stay grey under hue and saturation.",
    params: &[
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
    Ok(Box::new(Hsl {
        // Hue in sixths of a turn, the unit the conversion works in.
        turn: p.int("hue") as f64 / 60.0,
        sat: 1.0 + p.int("saturation") as f64 / 100.0,
        light: p.int("lightness") as f64 / 100.0,
    }))
}

struct Hsl {
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

impl Effect for Hsl {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        per_pixel(w, |px| {
            let f = |v: u8| v as f64 / 255.0;
            let (h, s, l) = to_hsl(f(px[0]), f(px[1]), f(px[2]));
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
            px[0] = to_byte(r);
            px[1] = to_byte(g);
            px[2] = to_byte(b);
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
