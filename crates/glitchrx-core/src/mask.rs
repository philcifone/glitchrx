//! Step masks: where a step's result shows (SPEC 9.3).
//!
//! A mask works like a layer mask in Photoshop: after a step runs, its
//! result is blended over the step's input pixel by pixel, fully where the
//! mask is 1, not at all where it is 0. Masks are stored in the recipe as a
//! short list of operations, not as an image, so they are small, exact at
//! any resolution, and replay from the command line:
//!
//! - **brush**: a set of brush strokes (paths of points with a size and
//!   hardness); a stroke can also erase the brush's own paint;
//! - **linear**: a gradient, 1 at `from` fading to 0 at `to`;
//! - **radial**: a circle centred on `from` and reaching `to`, hard-edged
//!   unless given a lower `hardness` (soften it with the mask's feather);
//! - **brightness**: the pixels whose brightness lies in a range, read from
//!   the picture as it arrives at the step.
//!
//! Starting from an empty mask (nothing shows), each operation (a
//! *component*, in the GUI) is combined in order with its `mode`: **add**
//! paints toward 1 and **subtract** toward 0, as painting white or black onto
//! a layer mask does, and **intersect** keeps only what the shape also
//! covers. Then the whole mask may be inverted and feathered (blurred).
//!
//! Each component stays separate, so any of them can be adjusted later: a
//! brush's eraser only takes back that brush's own paint. To cut into
//! everything else, add a brush that subtracts.
//!
//! Positions are fractions of the image (x of the width, y of the height)
//! and lengths fractions of its longer side, so the same mask lands on the
//! same place in a phone preview and in the full-size export. All the math
//! is basic f64 operations (and `sqrt`, which IEEE also specifies exactly),
//! so masks are as deterministic as the effects (SPEC D-25).

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::recipe::Cancel;

/// How an operation combines with the mask so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Paint toward 1: `m + v - m v` (the brush).
    #[default]
    Add,
    /// Paint toward 0: `m (1 - v)` (the eraser).
    Subtract,
    /// Keep only where the shape is too: `m v`.
    Intersect,
}

fn default_hardness() -> f64 {
    0.5
}

fn hard() -> f64 {
    1.0
}

/// One stroke of a brush.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Take back this brush's own paint instead of adding to it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub erase: bool,
    /// Diameter, as a share of the longer side.
    pub size: f64,
    /// 1 = crisp edge, 0 = soft all the way in.
    #[serde(default = "default_hardness")]
    pub hardness: f64,
    /// Flat list of `x, y` pairs along the stroke.
    pub points: Vec<f64>,
}

/// One operation (component) of a mask. In a recipe file:
///
/// ```toml
/// [[step.mask.op]]
/// shape = "brush"
/// mode = "add"
///
/// [[step.mask.op.stroke]]
/// size = 0.06          # brush diameter, share of the longer side
/// hardness = 0.5
/// points = [0.21, 0.40, 0.25, 0.43, 0.30, 0.47]   # x, y, x, y, ...
///
/// [[step.mask.op.stroke]]
/// erase = true
/// size = 0.02
/// points = [0.24, 0.42]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
pub enum MaskOp {
    /// Brush strokes, built up (and erased) on their own, then combined
    /// with the mask as one shape.
    Brush {
        #[serde(default)]
        mode: Mode,
        #[serde(rename = "stroke", default, skip_serializing_if = "Vec::is_empty")]
        strokes: Vec<Stroke>,
    },
    Linear {
        #[serde(default)]
        mode: Mode,
        from: [f64; 2],
        to: [f64; 2],
    },
    /// A circle. Hard-edged by default, so the mask's feather decides how
    /// soft it is (Phil, 2026-11-22: "the feather should be what makes it
    /// fade"); a lower `hardness` fades it toward the centre, like a brush.
    Radial {
        #[serde(default)]
        mode: Mode,
        /// Centre.
        from: [f64; 2],
        /// A point on the edge.
        to: [f64; 2],
        #[serde(default = "hard")]
        hardness: f64,
    },
    Brightness {
        #[serde(default)]
        mode: Mode,
        /// Brightness range that is fully on, 0..1.
        low: f64,
        high: f64,
        /// Width of the fade on either side of the range.
        #[serde(default)]
        soft: f64,
    },
}

impl MaskOp {
    pub fn mode(&self) -> Mode {
        match self {
            MaskOp::Brush { mode, .. }
            | MaskOp::Linear { mode, .. }
            | MaskOp::Radial { mode, .. }
            | MaskOp::Brightness { mode, .. } => *mode,
        }
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// A step's mask.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Mask {
    /// Swap shown and hidden: paint where the glitch should *not* be.
    #[serde(default, skip_serializing_if = "is_false")]
    pub invert: bool,
    /// Blur of the finished mask (its edges), as a share of the longer side.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub feather: f64,
    #[serde(rename = "op", default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<MaskOp>,
}

/// Limits that keep a hostile or broken recipe from eating the machine.
const MAX_OPS: usize = 2000;
const MAX_POINTS: usize = 400_000;

impl Mask {
    /// Check every value; a bad mask fails before any work.
    pub fn validate(&self) -> Result<()> {
        let bad = |msg: String| Err(Error::Recipe(format!("mask: {msg}")));
        let unit = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        // Positions may sit a little outside the picture (a stroke that
        // runs off the edge), but not absurdly far.
        let pos = |v: f64| v.is_finite() && (-4.0..=5.0).contains(&v);
        if !(self.feather.is_finite() && (0.0..=0.25).contains(&self.feather)) {
            return bad(format!("feather {} must be 0..0.25", self.feather));
        }
        if self.ops.len() > MAX_OPS {
            return bad(format!("{} operations (at most {MAX_OPS})", self.ops.len()));
        }
        let mut points = 0;
        for op in &self.ops {
            match op {
                MaskOp::Brush { strokes, .. } => {
                    for Stroke {
                        size,
                        hardness,
                        points: p,
                        ..
                    } in strokes
                    {
                        if !(size.is_finite() && *size > 0.0 && *size <= 1.0) {
                            return bad(format!("stroke size {size} must be above 0, at most 1"));
                        }
                        if !unit(*hardness) {
                            return bad(format!("stroke hardness {hardness} must be 0..1"));
                        }
                        if p.is_empty() || p.len() % 2 != 0 || !p.iter().all(|&v| pos(v)) {
                            return bad("stroke points must be x, y pairs near the picture".into());
                        }
                        points += p.len() / 2;
                    }
                }
                MaskOp::Linear { from, to, .. } => {
                    if !from.iter().chain(to).all(|&v| pos(v)) {
                        return bad("gradient ends must be near the picture".into());
                    }
                }
                MaskOp::Radial {
                    from, to, hardness, ..
                } => {
                    if !from.iter().chain(to).all(|&v| pos(v)) {
                        return bad("circle centre and edge must be near the picture".into());
                    }
                    if !unit(*hardness) {
                        return bad(format!("circle hardness {hardness} must be 0..1"));
                    }
                }
                MaskOp::Brightness {
                    low, high, soft, ..
                } => {
                    if !(unit(*low) && unit(*high) && unit(*soft) && low <= high) {
                        return bad(format!(
                            "brightness range {low}..{high} (soft {soft}) must be 0..1, low first"
                        ));
                    }
                }
            }
        }
        if points > MAX_POINTS {
            return bad(format!("{points} stroke points (at most {MAX_POINTS})"));
        }
        Ok(())
    }

    /// The mask for a `w` x `h` raster whose bytes (R, G, B per pixel) are
    /// `input`, the picture as it arrives at the step. One value per pixel,
    /// 0..1, in raster order.
    pub fn render(&self, w: usize, h: usize, input: &[u8], cancel: Cancel<'_>) -> Result<Vec<f32>> {
        let mut m = vec![0f32; w * h];
        // Lengths are shares of the longer side, in this raster's pixels.
        let long = w.max(h) as f64;
        for op in &self.ops {
            if cancel() {
                return Err(Error::Cancelled);
            }
            match op {
                MaskOp::Brush { mode, strokes } => brush(&mut m, w, h, *mode, long, strokes),
                MaskOp::Linear { mode, from, to } => {
                    let (a, d) = ends(from, to, w, h);
                    let len2 = d.0 * d.0 + d.1 * d.1;
                    each_pixel(&mut m, w, *mode, |px, py| {
                        if len2 < 1e-9 {
                            return 1.0;
                        }
                        let t = ((px - a.0) * d.0 + (py - a.1) * d.1) / len2;
                        1.0 - t.clamp(0.0, 1.0)
                    });
                }
                MaskOp::Radial {
                    mode,
                    from,
                    to,
                    hardness,
                } => {
                    let (a, d) = ends(from, to, w, h);
                    let r = (d.0 * d.0 + d.1 * d.1).sqrt();
                    let edge = edge_width(r, *hardness);
                    each_pixel(&mut m, w, *mode, |px, py| {
                        let (dx, dy) = (px - a.0, py - a.1);
                        soft_edge(r, (dx * dx + dy * dy).sqrt(), edge)
                    });
                }
                MaskOp::Brightness {
                    mode,
                    low,
                    high,
                    soft,
                } => {
                    let (low, high, soft) = (*low, *high, *soft);
                    for (i, v) in m.iter_mut().enumerate() {
                        let p = &input[i * 3..i * 3 + 3];
                        let l =
                            (0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64)
                                / 255.0;
                        let c = if soft > 0.0 {
                            let up = ((l - (low - soft)) / soft).clamp(0.0, 1.0);
                            let down = (((high + soft) - l) / soft).clamp(0.0, 1.0);
                            up.min(down)
                        } else if low <= l && l <= high {
                            1.0
                        } else {
                            0.0
                        };
                        *v = combine(*v as f64, c, *mode) as f32;
                    }
                }
            }
        }
        if self.invert {
            for v in &mut m {
                *v = 1.0 - *v;
            }
        }
        // Three box blurs in a row approximate a Gaussian; a box of radius
        // k three times has a standard deviation of about k.
        let k = (self.feather * long).round() as usize;
        if k > 0 {
            for _ in 0..3 {
                if cancel() {
                    return Err(Error::Cancelled);
                }
                blur_rows(&mut m, w, h, k);
                blur_columns(&mut m, w, h, k);
            }
        }
        Ok(m)
    }
}

/// Combine the mask so far with an operation's value at one pixel.
fn combine(m: f64, v: f64, mode: Mode) -> f64 {
    match mode {
        Mode::Add => m + v - m * v,
        Mode::Subtract => m * (1.0 - v),
        Mode::Intersect => m * v,
    }
}

/// Width of the soft edge of a round shape of radius `r` (pixels). Even the
/// hardest edge keeps a pixel of softness, which is anti-aliasing.
fn edge_width(r: f64, hardness: f64) -> f64 {
    (r * (1.0 - hardness)).max(1.0)
}

/// Coverage at distance `d` from the centre of a round shape of radius
/// `r`: 1 inside, 0 outside, and a smoothstep across the `edge`, a fade
/// that starts and ends flat, so no ring shows.
fn soft_edge(r: f64, d: f64, edge: f64) -> f64 {
    let s = ((r - d) / edge).clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

/// Gradient ends in pixels: the start, and the vector to the end.
fn ends(from: &[f64; 2], to: &[f64; 2], w: usize, h: usize) -> ((f64, f64), (f64, f64)) {
    let (w, h) = (w as f64, h as f64);
    let a = (from[0] * w, from[1] * h);
    (a, (to[0] * w - a.0, to[1] * h - a.1))
}

/// Apply `f(x, y)` (pixel centres) to every pixel with `mode`.
fn each_pixel(m: &mut [f32], w: usize, mode: Mode, f: impl Fn(f64, f64) -> f64) {
    for (i, v) in m.iter_mut().enumerate() {
        let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        *v = combine(*v as f64, f(x, y), mode) as f32;
    }
}

/// A rectangle of pixels, `x0..x1` by `y0..y1`.
#[derive(Clone, Copy)]
struct Rect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Rect {
    fn union(self, o: Rect) -> Rect {
        Rect {
            x0: self.x0.min(o.x0),
            y0: self.y0.min(o.y0),
            x1: self.x1.max(o.x1),
            y1: self.y1.max(o.y1),
        }
    }

    fn width(self) -> usize {
        self.x1 - self.x0
    }
}

/// A stroke's points in pixels and its radius.
fn stroke_geometry(s: &Stroke, w: usize, h: usize, long: f64) -> (Vec<(f64, f64)>, f64) {
    let (wf, hf) = (w as f64, h as f64);
    let pts = s
        .points
        .chunks_exact(2)
        .map(|p| (p[0] * wf, p[1] * hf))
        .collect();
    (pts, (s.size * long * 0.5).max(0.5))
}

/// The pixels a stroke can touch, clipped to the raster; none if it lies
/// entirely outside.
fn stroke_rect(pts: &[(f64, f64)], radius: f64, w: usize, h: usize) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        x0 = x0.min(x - radius);
        y0 = y0.min(y - radius);
        x1 = x1.max(x + radius);
        y1 = y1.max(y + radius);
    }
    let clip = |v: f64, n: usize| (v.floor().max(0.0) as usize).min(n);
    let r = Rect {
        x0: clip(x0, w),
        y0: clip(y0, h),
        x1: clip(x1 + 1.0, w),
        y1: clip(y1 + 1.0, h),
    };
    (r.x0 < r.x1 && r.y0 < r.y1).then_some(r)
}

/// One stroke's coverage inside `rect`: the union of round-capped segments
/// between its points, with a soft edge. Each pixel takes the maximum over
/// segments, so joints are not painted twice.
fn stroke_coverage(pts: &[(f64, f64)], radius: f64, hardness: f64, rect: Rect) -> Vec<f32> {
    let edge = edge_width(radius, hardness);
    let bw = rect.width();
    let mut cov = vec![0f32; bw * (rect.y1 - rect.y0)];
    let segments: Vec<((f64, f64), (f64, f64))> = if pts.len() == 1 {
        vec![(pts[0], pts[0])]
    } else {
        pts.windows(2).map(|s| (s[0], s[1])).collect()
    };
    let clip = |v: f64, lo: usize, hi: usize| (v.floor().max(0.0) as usize).clamp(lo, hi);
    for (a, b) in segments {
        let sx0 = clip(a.0.min(b.0) - radius, rect.x0, rect.x1);
        let sy0 = clip(a.1.min(b.1) - radius, rect.y0, rect.y1);
        let sx1 = clip(a.0.max(b.0) + radius + 1.0, rect.x0, rect.x1);
        let sy1 = clip(a.1.max(b.1) + radius + 1.0, rect.y0, rect.y1);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len2 = dx * dx + dy * dy;
        for y in sy0..sy1 {
            let py = y as f64 + 0.5;
            for x in sx0..sx1 {
                let px = x as f64 + 0.5;
                // Distance from the pixel centre to the segment.
                let t = if len2 > 0.0 {
                    (((px - a.0) * dx + (py - a.1) * dy) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (ex, ey) = (px - (a.0 + t * dx), py - (a.1 + t * dy));
                let d = (ex * ex + ey * ey).sqrt();
                let v = soft_edge(radius, d, edge) as f32;
                let c = &mut cov[(y - rect.y0) * bw + (x - rect.x0)];
                if v > *c {
                    *c = v;
                }
            }
        }
    }
    cov
}

/// Paint a brush component: its strokes are built up (and erased) on their
/// own, inside the brush's bounding box, and the result is combined with
/// the mask once, with the brush's `mode`.
fn brush(m: &mut [f32], w: usize, h: usize, mode: Mode, long: f64, strokes: &[Stroke]) {
    let shapes: Vec<_> = strokes
        .iter()
        .filter_map(|s| {
            let (pts, radius) = stroke_geometry(s, w, h, long);
            stroke_rect(&pts, radius, w, h).map(|r| (s, pts, radius, r))
        })
        .collect();
    let rect = shapes.iter().map(|s| s.3).reduce(Rect::union);

    if mode == Mode::Intersect {
        // Outside the brush its coverage is 0, which clears the mask there.
        for (i, v) in m.iter_mut().enumerate() {
            let (x, y) = (i % w, i / w);
            let inside = rect.is_some_and(|r| x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1);
            if !inside {
                *v = 0.0;
            }
        }
    }
    let Some(rect) = rect else { return };
    let bw = rect.width();
    let mut cov = vec![0f32; bw * (rect.y1 - rect.y0)];
    for (s, pts, radius, r) in &shapes {
        let one = stroke_coverage(pts, *radius, s.hardness, *r);
        let how = if s.erase { Mode::Subtract } else { Mode::Add };
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let v = one[(y - r.y0) * r.width() + (x - r.x0)] as f64;
                let c = &mut cov[(y - rect.y0) * bw + (x - rect.x0)];
                *c = combine(*c as f64, v, how) as f32;
            }
        }
    }
    for y in rect.y0..rect.y1 {
        for x in rect.x0..rect.x1 {
            let v = cov[(y - rect.y0) * bw + (x - rect.x0)] as f64;
            let i = y * w + x;
            m[i] = combine(m[i] as f64, v, mode) as f32;
        }
    }
}

/// Box blur of radius `k` along each row, edges extended.
fn blur_rows(m: &mut [f32], w: usize, h: usize, k: usize) {
    let mut line = vec![0f32; w];
    let n = (2 * k + 1) as f64;
    for y in 0..h {
        let row = &mut m[y * w..(y + 1) * w];
        line.copy_from_slice(row);
        let at = |i: isize| line[i.clamp(0, w as isize - 1) as usize] as f64;
        // Running sum of the window centred on x.
        let mut s: f64 = (-(k as isize)..=k as isize).map(at).sum();
        for (x, out) in row.iter_mut().enumerate() {
            *out = (s / n) as f32;
            let x = x as isize;
            s += at(x + k as isize + 1) - at(x - k as isize);
        }
    }
}

/// Box blur of radius `k` down each column, edges extended. Works a row at
/// a time with one running sum per column, so memory is read in order.
fn blur_columns(m: &mut [f32], w: usize, h: usize, k: usize) {
    let src = m.to_vec();
    let n = (2 * k + 1) as f64;
    let row = |y: isize| {
        let y = y.clamp(0, h as isize - 1) as usize;
        &src[y * w..(y + 1) * w]
    };
    let mut s = vec![0f64; w];
    for y in -(k as isize)..=k as isize {
        for (acc, &v) in s.iter_mut().zip(row(y)) {
            *acc += v as f64;
        }
    }
    for y in 0..h {
        for (out, acc) in m[y * w..(y + 1) * w].iter_mut().zip(&s) {
            *out = (acc / n) as f32;
        }
        let (add, sub) = (
            row(y as isize + k as isize + 1),
            row(y as isize - k as isize),
        );
        for ((acc, &a), &b) in s.iter_mut().zip(add).zip(sub) {
            *acc += a as f64 - b as f64;
        }
    }
}

/// Blend a step's result over its input: `after = before + (after -
/// before) * coverage * opacity`, per pixel, rounded to the nearest byte.
/// Without a mask, coverage is 1 everywhere.
pub fn composite(before: &[u8], after: &mut [u8], mask: Option<&[f32]>, opacity: f64) {
    for (i, (b, a)) in before
        .chunks_exact(3)
        .zip(after.chunks_exact_mut(3))
        .enumerate()
    {
        let m = mask.map_or(1.0, |c| c[i] as f64) * opacity;
        if m >= 1.0 {
            continue;
        }
        if m <= 0.0 {
            a.copy_from_slice(b);
            continue;
        }
        for (av, &bv) in a.iter_mut().zip(b) {
            let v = bv as f64 + (*av as f64 - bv as f64) * m;
            *av = (v + 0.5).floor() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::NEVER;

    fn grey(w: usize, h: usize, v: u8) -> Vec<u8> {
        vec![v; w * h * 3]
    }

    fn at(m: &[f32], w: usize, x: usize, y: usize) -> f32 {
        m[y * w + x]
    }

    /// A hard-edged single-point stroke.
    fn dab(erase: bool, x: f64, y: f64, size: f64) -> Stroke {
        Stroke {
            erase,
            size,
            hardness: 1.0,
            points: vec![x, y],
        }
    }

    /// A brush component with one dab.
    fn dot(mode: Mode, x: f64, y: f64, size: f64) -> MaskOp {
        MaskOp::Brush {
            mode,
            strokes: vec![dab(false, x, y, size)],
        }
    }

    #[test]
    fn empty_mask_hides_everything_and_invert_shows_it() {
        let m = Mask::default().render(4, 4, &grey(4, 4, 0), NEVER).unwrap();
        assert!(m.iter().all(|&v| v == 0.0));
        let inv = Mask {
            invert: true,
            ..Mask::default()
        };
        assert!(
            inv.render(4, 4, &grey(4, 4, 0), NEVER)
                .unwrap()
                .iter()
                .all(|&v| v == 1.0)
        );
    }

    #[test]
    fn a_stroke_covers_its_path_and_nothing_else() {
        // A horizontal stroke across the middle of a 40 x 20 raster.
        let mask = Mask {
            ops: vec![MaskOp::Brush {
                mode: Mode::Add,
                strokes: vec![Stroke {
                    erase: false,
                    size: 0.1, // 4 px wide
                    hardness: 1.0,
                    points: vec![0.1, 0.5, 0.9, 0.5],
                }],
            }],
            ..Mask::default()
        };
        let m = mask.render(40, 20, &grey(40, 20, 0), NEVER).unwrap();
        assert_eq!(at(&m, 40, 20, 10), 1.0);
        assert_eq!(at(&m, 40, 20, 2), 0.0);
        assert_eq!(at(&m, 40, 1, 10), 0.0);
    }

    #[test]
    fn the_eraser_takes_back_its_own_brush_only() {
        // A radial underneath, then a brush whose eraser crosses both.
        let mask = Mask {
            ops: vec![
                MaskOp::Radial {
                    mode: Mode::Add,
                    from: [0.25, 0.5],
                    to: [0.4, 0.5],
                    hardness: 1.0,
                },
                MaskOp::Brush {
                    mode: Mode::Add,
                    strokes: vec![
                        dab(false, 0.75, 0.5, 0.5),
                        // An erase line through both shapes.
                        Stroke {
                            erase: true,
                            size: 0.1,
                            hardness: 1.0,
                            points: vec![0.05, 0.5, 0.95, 0.5],
                        },
                    ],
                },
            ],
            ..Mask::default()
        };
        let m = mask.render(40, 20, &grey(40, 20, 0), NEVER).unwrap();
        assert_eq!(at(&m, 40, 30, 10), 0.0, "the brush's own paint is erased");
        assert_eq!(at(&m, 40, 30, 5), 1.0, "the rest of the brush stays");
        assert_eq!(at(&m, 40, 10, 10), 1.0, "the radial is untouched");
    }

    #[test]
    fn a_subtracting_brush_cuts_into_everything() {
        let mask = Mask {
            ops: vec![
                MaskOp::Radial {
                    mode: Mode::Add,
                    from: [0.5, 0.5],
                    to: [1.0, 0.5],
                    hardness: 1.0,
                },
                dot(Mode::Subtract, 0.5, 0.5, 0.2),
            ],
            ..Mask::default()
        };
        let m = mask.render(20, 20, &grey(20, 20, 0), NEVER).unwrap();
        assert_eq!(at(&m, 20, 10, 10), 0.0, "cut out");
        assert_eq!(at(&m, 20, 10, 6), 1.0, "radial around it");
    }

    #[test]
    fn an_empty_brush_changes_nothing_unless_it_intersects() {
        let base = vec![MaskOp::Radial {
            mode: Mode::Add,
            from: [0.5, 0.5],
            to: [1.0, 0.5],
            hardness: 1.0,
        }];
        let with = |mode| Mask {
            ops: [
                base.clone(),
                vec![MaskOp::Brush {
                    mode,
                    strokes: vec![],
                }],
            ]
            .concat(),
            ..Mask::default()
        };
        let plain = Mask {
            ops: base.clone(),
            ..Mask::default()
        }
        .render(10, 10, &grey(10, 10, 0), NEVER)
        .unwrap();
        let r = |m: Mask| m.render(10, 10, &grey(10, 10, 0), NEVER).unwrap();
        assert_eq!(r(with(Mode::Add)), plain);
        assert_eq!(r(with(Mode::Subtract)), plain);
        assert!(r(with(Mode::Intersect)).iter().all(|&v| v == 0.0));
    }

    #[test]
    fn linear_gradient_fades_from_start_to_end() {
        let mask = Mask {
            ops: vec![MaskOp::Linear {
                mode: Mode::Add,
                from: [0.0, 0.5],
                to: [1.0, 0.5],
            }],
            ..Mask::default()
        };
        let m = mask.render(10, 2, &grey(10, 2, 0), NEVER).unwrap();
        let row: Vec<f32> = (0..10).map(|x| at(&m, 10, x, 0)).collect();
        assert!(row.windows(2).all(|p| p[0] > p[1]), "{row:?}");
        assert!((row[0] - 0.95).abs() < 1e-6 && (row[9] - 0.05).abs() < 1e-6);
    }

    #[test]
    fn radial_is_a_hard_circle_unless_softened() {
        let circle = |hardness: f64| Mask {
            ops: vec![MaskOp::Radial {
                mode: Mode::Add,
                from: [0.5, 0.5],
                to: [0.75, 0.5], // radius 10 px on a 40 px square
                hardness,
            }],
            ..Mask::default()
        };
        let hard = circle(1.0).render(40, 40, &grey(40, 40, 0), NEVER).unwrap();
        // Full right up to the edge, nothing past it: no fade of its own.
        assert_eq!(at(&hard, 40, 20, 20), 1.0);
        assert_eq!(at(&hard, 40, 28, 20), 1.0);
        assert_eq!(at(&hard, 40, 31, 20), 0.0);
        assert_eq!(at(&hard, 40, 0, 0), 0.0);
        // A lower hardness fades toward the centre, like a soft brush.
        let soft = circle(0.0).render(40, 40, &grey(40, 40, 0), NEVER).unwrap();
        let v = at(&soft, 40, 26, 20);
        assert!(v > 0.05 && v < 0.6, "{v}");
        // A recipe that leaves hardness out gets the hard circle.
        let op: MaskOp =
            toml::from_str("shape = \"radial\"\nfrom = [0.5, 0.5]\nto = [0.6, 0.5]").unwrap();
        assert!(matches!(op, MaskOp::Radial { hardness, .. } if hardness == 1.0));
    }

    #[test]
    fn brightness_selects_by_level() {
        let mut input = grey(2, 1, 30);
        input[3..6].copy_from_slice(&[230, 230, 230]);
        let mask = Mask {
            ops: vec![MaskOp::Brightness {
                mode: Mode::Add,
                low: 0.5,
                high: 1.0,
                soft: 0.0,
            }],
            ..Mask::default()
        };
        assert_eq!(mask.render(2, 1, &input, NEVER).unwrap(), vec![0.0, 1.0]);
    }

    #[test]
    fn intersect_keeps_only_the_overlap() {
        let mask = Mask {
            ops: vec![
                MaskOp::Linear {
                    mode: Mode::Add,
                    from: [0.0, 0.0],
                    to: [0.0, 0.0],
                },
                dot(Mode::Intersect, 0.25, 0.5, 0.3),
            ],
            ..Mask::default()
        };
        let m = mask.render(20, 10, &grey(20, 10, 0), NEVER).unwrap();
        assert_eq!(at(&m, 20, 5, 5), 1.0);
        assert_eq!(at(&m, 20, 15, 5), 0.0);
    }

    #[test]
    fn feather_softens_edges_but_keeps_the_middle() {
        let hard = Mask {
            ops: vec![MaskOp::Linear {
                mode: Mode::Add,
                from: [0.5, 0.0],
                to: [0.5001, 0.0],
            }],
            ..Mask::default()
        };
        let soft = Mask {
            feather: 0.05,
            ..hard.clone()
        };
        let (w, h) = (100, 4);
        let a = hard.render(w, h, &grey(w, h, 0), NEVER).unwrap();
        let b = soft.render(w, h, &grey(w, h, 0), NEVER).unwrap();
        // Left half on, right half off; feathered, the edge ramps.
        assert_eq!((at(&a, w, 48, 1), at(&a, w, 52, 1)), (1.0, 0.0));
        let (l, r) = (at(&b, w, 48, 1), at(&b, w, 52, 1));
        assert!(l < 1.0 && l > 0.5 && r > 0.0 && r < 0.5, "{l} {r}");
        assert!(at(&b, w, 5, 1) > 0.999 && at(&b, w, 95, 1) < 0.001);
    }

    #[test]
    fn a_preview_sized_mask_matches_the_full_one() {
        let mask = Mask {
            feather: 0.02,
            ops: vec![
                MaskOp::Brush {
                    mode: Mode::Add,
                    strokes: vec![
                        Stroke {
                            erase: false,
                            size: 0.08,
                            hardness: 0.3,
                            points: vec![0.1, 0.2, 0.5, 0.6, 0.8, 0.3],
                        },
                        Stroke {
                            erase: true,
                            size: 0.03,
                            hardness: 0.6,
                            points: vec![0.45, 0.5, 0.55, 0.65],
                        },
                    ],
                },
                MaskOp::Radial {
                    mode: Mode::Subtract,
                    from: [0.5, 0.6],
                    to: [0.6, 0.6],
                    hardness: 1.0,
                },
            ],
            ..Mask::default()
        };
        let full = mask.render(400, 300, &grey(400, 300, 0), NEVER).unwrap();
        let small = mask.render(100, 75, &grey(100, 75, 0), NEVER).unwrap();
        // Average the full mask over each 4x4 block and compare.
        let mut err = 0.0;
        for y in 0..75 {
            for x in 0..100 {
                let mut s = 0.0;
                for dy in 0..4 {
                    for dx in 0..4 {
                        s += at(&full, 400, x * 4 + dx, y * 4 + dy) as f64;
                    }
                }
                err += (s / 16.0 - at(&small, 100, x, y) as f64).abs();
            }
        }
        let mean = err / (100.0 * 75.0);
        assert!(mean < 0.02, "mean difference {mean}");
    }

    #[test]
    fn composite_blends_by_coverage_and_opacity() {
        let before = vec![0, 0, 0, 100, 100, 100];
        let mut after = vec![200, 200, 200, 200, 200, 200];
        composite(&before, &mut after, Some(&[1.0, 0.5]), 1.0);
        assert_eq!(after, vec![200, 200, 200, 150, 150, 150]);
        let mut after = vec![200, 200, 200, 200, 200, 200];
        composite(&before, &mut after, None, 0.25);
        assert_eq!(after, vec![50, 50, 50, 125, 125, 125]);
    }

    #[test]
    fn validation_catches_bad_values() {
        let bad = |op: MaskOp| {
            Mask {
                ops: vec![op],
                ..Mask::default()
            }
            .validate()
            .is_err()
        };
        assert!(bad(dot(Mode::Add, 0.5, 0.5, 0.0)));
        assert!(bad(MaskOp::Brush {
            mode: Mode::Add,
            strokes: vec![Stroke {
                erase: false,
                size: 0.1,
                hardness: 0.5,
                points: vec![0.5],
            }],
        }));
        assert!(bad(MaskOp::Brightness {
            mode: Mode::Add,
            low: 0.8,
            high: 0.2,
            soft: 0.0,
        }));
        assert!(bad(dot(Mode::Add, f64::NAN, 0.5, 0.1)));
        assert!(!bad(dot(Mode::Add, 0.5, 0.5, 0.1)));
        assert!(
            Mask {
                feather: 0.9,
                ..Mask::default()
            }
            .validate()
            .is_err()
        );
    }
}
