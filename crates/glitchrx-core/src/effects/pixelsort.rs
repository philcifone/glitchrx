//! `pixelsort`: classic pixel sorting, after Kim Asendorf's ASDFPixelSort.
//!
//! Unlike every other effect, this one is not audio: it sees the window as
//! whole RGB pixels in two dimensions. Each column (or row) is cut into
//! *runs* of pixels whose brightness lies inside a band; pixels outside the
//! band stay where they are and break the runs. Every run is then sorted by
//! a key. Bright areas melt into streaks while shadows (or highlights) hold
//! the picture's structure, which is what makes the look.

use super::{Effect, EffectInfo, Group, PlacementDefaults, Window};
use crate::error::{Error, Result};
use crate::params::{Bound, Kind, ParamSpec, Params};
use crate::rng::Rng;

pub static INFO: EffectInfo = EffectInfo {
    id: "pixelsort",
    group: Group::Pixel,
    version: 1,
    upgrades: &[],
    audacity: "none (a pixel effect)",
    summary: "Sort runs of pixels by brightness, hue or saturation: streaks that melt down or across the picture.",
    details: "Each column (or row, with `direction=horizontal`) is cut into runs of pixels \
              whose brightness lies between `low` and `high`; pixels outside that band stay \
              put and end the runs. Every run is then sorted by `key`: `descending` puts the \
              highest value first (at the top, or on the left), `ascending` the lowest. \
              Raise `low` to let more shadows hold the picture together; lower it to melt \
              more. `max` caps the length of a run (`%` is of the column or row, so \
              `100%` is no cap), which cuts long smooth areas into repeated streaks.",
    params: &[
        ParamSpec {
            name: "direction",
            kind: Kind::Choice(&["vertical", "horizontal"]),
            default: "vertical",
            help: "sort down the columns or along the rows",
        },
        ParamSpec {
            name: "key",
            kind: Kind::Choice(&["brightness", "hue", "saturation"]),
            default: "brightness",
            help: "what the pixels in a run are sorted by",
        },
        ParamSpec {
            name: "order",
            kind: Kind::Choice(&["descending", "ascending"]),
            default: "descending",
            help: "descending puts the highest first (top or left)",
        },
        ParamSpec {
            name: "low",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.25",
            help: "darkest brightness that is sorted; darker pixels stay and break runs",
        },
        ParamSpec {
            name: "high",
            kind: Kind::Float { min: 0.0, max: 1.0 },
            default: "0.9",
            help: "brightest brightness that is sorted; brighter pixels stay and break runs",
        },
        ParamSpec {
            name: "max",
            kind: Kind::Size,
            default: "100%",
            help: "longest run in px or % of the column or row (100% = no limit)",
        },
    ],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "10%",
    },
    random_within: false,
    // Where runs break depends on fine texture, which a shrunken preview
    // averages away, so preview runs come out longer (measured error 22.8
    // against strength 40.7). Sorting is fast (about 0.4 s for 24 MP), so
    // the GUI simply renders it exactly once the user pauses.
    exact_preview: true,
    strokes: false,
    example: "glitchrx --pixelsort low=0.3,region=25%:100% photo.jpg",
    build,
};

#[derive(Clone, Copy, PartialEq)]
enum Key {
    Brightness,
    Hue,
    Saturation,
}

fn build(p: &Params) -> Result<Box<dyn Effect>> {
    let key = match p.choice("key") {
        "hue" => Key::Hue,
        "saturation" => Key::Saturation,
        _ => Key::Brightness,
    };
    // Thresholds in the integer brightness scale used below, so the band
    // test is exact integer arithmetic.
    let scale = |v: f64| (v * LUMA_MAX as f64).round() as u32;
    Ok(Box::new(PixelSort {
        vertical: p.choice("direction") == "vertical",
        key,
        descending: p.choice("order") == "descending",
        low: scale(p.float("low")),
        high: scale(p.float("high")),
        max: p.size("max"),
    }))
}

struct PixelSort {
    vertical: bool,
    key: Key,
    descending: bool,
    low: u32,
    high: u32,
    max: Bound,
}

/// Brightness as an exact integer: Rec. 709 luma weights times 10 000, so
/// white is 2 550 000. Integers keep the band test and the sort the same on
/// every machine.
const LUMA_MAX: u32 = 255 * 10_000;

fn luma(p: &[u8]) -> u32 {
    2126 * p[0] as u32 + 7152 * p[1] as u32 + 722 * p[2] as u32
}

/// HSV hue in 0..6 (red, yellow, green, cyan, blue, magenta), from basic
/// operations only. Greys have no hue and sort as red (0).
fn hue(p: &[u8]) -> f64 {
    let (r, g, b) = (p[0] as f64, p[1] as f64, p[2] as f64);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let c = max - min;
    if c == 0.0 {
        return 0.0;
    }
    let h = if max == r {
        (g - b) / c
    } else if max == g {
        (b - r) / c + 2.0
    } else {
        (r - g) / c + 4.0
    };
    if h < 0.0 { h + 6.0 } else { h }
}

/// HSV saturation in 0..1.
fn saturation(p: &[u8]) -> f64 {
    let max = p[0].max(p[1]).max(p[2]) as f64;
    let min = p[0].min(p[1]).min(p[2]) as f64;
    if max == 0.0 { 0.0 } else { (max - min) / max }
}

impl PixelSort {
    fn key_of(&self, p: &[u8]) -> f64 {
        match self.key {
            Key::Brightness => luma(p) as f64,
            Key::Hue => hue(p),
            Key::Saturation => saturation(p),
        }
    }

    /// Sort one line of pixels, given as pixel indices into `bytes` in
    /// order along the line. `cap` is the longest run.
    fn sort_line(
        &self,
        bytes: &mut [u8],
        line: &[usize],
        cap: usize,
        buf: &mut Vec<(f64, [u8; 3])>,
    ) {
        // Which pixels are in the band, decided before anything moves.
        // (Sorting only permutes pixels inside one run, all in the band, so
        // the answer would not change anyway; this just keeps the borrows
        // simple.)
        let in_band: Vec<bool> = line
            .iter()
            .map(|&i| {
                let l = luma(&bytes[i * 3..i * 3 + 3]);
                self.low <= l && l <= self.high
            })
            .collect();
        let mut s = 0;
        while s < line.len() {
            if !in_band[s] {
                s += 1;
                continue;
            }
            let mut e = s + 1;
            while e < line.len() && in_band[e] {
                e += 1;
            }
            // The run is line[s..e]; sort it in pieces of at most `cap`.
            let mut a = s;
            while a < e {
                let b = (a + cap.max(1)).min(e);
                self.sort_run(bytes, &line[a..b], buf);
                a = b;
            }
            s = e;
        }
    }

    fn sort_run(&self, bytes: &mut [u8], run: &[usize], buf: &mut Vec<(f64, [u8; 3])>) {
        if run.len() < 2 {
            return;
        }
        buf.clear();
        for &i in run {
            let p = [bytes[i * 3], bytes[i * 3 + 1], bytes[i * 3 + 2]];
            buf.push((self.key_of(&p), p));
        }
        // A stable sort, so equal keys keep their order: same result on
        // every platform and every run.
        if self.descending {
            buf.sort_by(|x, y| y.0.total_cmp(&x.0));
        } else {
            buf.sort_by(|x, y| x.0.total_cmp(&y.0));
        }
        for (&i, (_, p)) in run.iter().zip(buf.iter()) {
            bytes[i * 3..i * 3 + 3].copy_from_slice(p);
        }
    }
}

impl Effect for PixelSort {
    fn run(&self, w: &mut Window<'_>, _rng: &mut Rng) -> Result<()> {
        let width = w.units.width.max(1);
        // The window as pixel indices into the raster: p0..p1. Windows of
        // pixel effects start and end on whole pixels.
        let p0 = w.start / 3;
        let p1 = p0 + w.bytes.len() / 3;
        if p1 <= p0 {
            return Ok(());
        }
        let mut line = Vec::new();
        let mut buf = Vec::new();
        // Each line is a run of raster pixels; `cap` follows its length.
        let cap = |len: usize| self.max.of_window(len * 3, w.units) / 3;

        let lines = if self.vertical {
            width
        } else {
            (p1 - 1) / width - p0 / width + 1
        };
        for n in 0..lines {
            if n % 256 == 0 && (w.cancel)() {
                return Err(Error::Cancelled);
            }
            line.clear();
            if self.vertical {
                // Column `x`: the rows whose pixel in that column lies in
                // the window. The window is contiguous in raster order, so
                // those rows are contiguous too.
                let x = n;
                let first = if p0 > x { (p0 - x).div_ceil(width) } else { 0 };
                if p1 - 1 < x {
                    continue;
                }
                let last = (p1 - 1 - x) / width;
                line.extend((first..=last).map(|y| y * width + x - p0));
            } else {
                // Row `y`, clipped to the window.
                let y = p0 / width + n;
                let a = (y * width).max(p0);
                let b = ((y + 1) * width).min(p1);
                line.extend((a..b).map(|i| i - p0));
            }
            let c = cap(line.len());
            self.sort_line(w.bytes, &line, c, &mut buf);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::testutil::*;
    use crate::samples::Encoding;

    /// A raster from greys, one byte value per pixel.
    fn greys(v: &[u8]) -> Vec<u8> {
        v.iter().flat_map(|&g| [g, g, g]).collect()
    }

    #[test]
    fn sorts_a_column_brightest_first() {
        // One pixel wide: the column is the whole raster. 10 is below the
        // band and stays, splitting the column into two runs.
        let e = make(&INFO, "");
        let mut b = greys(&[100, 200, 150, 10, 120, 180]);
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert_eq!(b, greys(&[200, 150, 100, 10, 180, 120]));
    }

    #[test]
    fn horizontal_ascending_sorts_each_row_alone() {
        let e = make(&INFO, "direction=horizontal,order=ascending");
        let mut b = greys(&[200, 100, 150, 90, 220, 120]);
        run(e.as_ref(), &mut b, 3, Encoding::U8);
        assert_eq!(b, greys(&[100, 150, 200, 90, 120, 220]));
    }

    #[test]
    fn vertical_sorts_columns_not_rows() {
        // 2 wide, 3 high: column 0 is 100,200,150; column 1 is 120,140,130.
        let e = make(&INFO, "");
        let mut b = greys(&[100, 120, 200, 140, 150, 130]);
        run(e.as_ref(), &mut b, 2, Encoding::U8);
        assert_eq!(b, greys(&[200, 140, 150, 130, 100, 120]));
    }

    #[test]
    fn max_cuts_runs_into_pieces() {
        let e = make(&INFO, "direction=horizontal,max=2px");
        let mut b = greys(&[100, 200, 110, 210]);
        run(e.as_ref(), &mut b, 4, Encoding::U8);
        assert_eq!(b, greys(&[200, 100, 210, 110]));
    }

    #[test]
    fn hue_key_orders_by_colour() {
        // Blue (hue 4), red (0), green (2), all bright enough to sort.
        let e = make(&INFO, "key=hue,order=ascending,low=0");
        let mut b = vec![0, 0, 255, 255, 0, 0, 0, 255, 0];
        run(e.as_ref(), &mut b, 1, Encoding::U8);
        assert_eq!(b, vec![255, 0, 0, 0, 255, 0, 0, 0, 255]);
    }

    #[test]
    fn a_window_inside_the_raster_finds_its_columns() {
        // 2 wide, 4 high; the window covers pixels 3..7 (rows 1.5 to 3).
        let units = crate::params::Units::full(2, 4);
        let e = make(&INFO, "");
        let mut raster = greys(&[0, 0, 0, 100, 200, 110, 150, 210]);
        let mut win = Window {
            bytes: &mut raster[9..24],
            start: 9,
            encoding: Encoding::U8,
            channels: 1,
            units: &units,
            cancel: &|| false,
        };
        e.run(&mut win, &mut Rng::new(1)).unwrap();
        // Column 0 in the window: pixels 4, 6 (200, 150); column 1: 3, 5, 7
        // (100, 110, 210). Pixels 0..3 are outside and untouched.
        assert_eq!(raster, greys(&[0, 0, 0, 210, 200, 110, 150, 100]));
    }
}
