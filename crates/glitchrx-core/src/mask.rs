//! Step masks: where a step's result shows (SPEC 9.3).
//!
//! A mask works like a layer mask in Photoshop: after a step runs, its
//! result is blended over the step's input pixel by pixel, fully where the
//! mask is 1, not at all where it is 0. Masks are stored in the recipe as a
//! short list of components, not as an image, so they are small, exact at
//! any resolution, and replay from the command line. A component's shape is
//! one of:
//!
//! - **brush**: a set of brush strokes (paths of points with a size and
//!   hardness); a stroke can also erase the brush's own paint;
//! - **linear**: a gradient, 1 at `from` fading to 0 at `to`;
//! - **radial**: a circle centred on `from` and reaching `to`, hard-edged
//!   unless given a lower `hardness` (soften it with its feather);
//! - **rect**: a rectangle, which may be turned about its centre;
//! - **path**: a closed outline drawn with the pen, of corners and smooth
//!   points (whose handles bend the sides into curves);
//! - **brightness**: the pixels whose brightness lies in a range, read from
//!   the picture as it arrives at the step.
//!
//! Rectangles and paths are filled with exact area coverage (`fill.rs`), so
//! their edges are anti-aliased and crisp; feather softens them.
//!
//! Each component has its own **feather** (a blur of its edges) and
//! **opacity** (how strongly it counts). Starting from an empty mask
//! (nothing shows), the components are combined in order with their `mode`:
//! **add** paints toward 1 and **subtract** toward 0, as painting white or
//! black onto a layer mask does, and **intersect** keeps only what the shape
//! also covers. Then the whole mask may be inverted.
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

use crate::dsp;
use crate::error::{Error, Result};
use crate::fill::{self, Pt};
use crate::recipe::Cancel;

/// How a component combines with the mask so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Paint toward 1: `m + v - m v`.
    #[default]
    Add,
    /// Paint toward 0: `m (1 - v)`.
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

fn one() -> f64 {
    1.0
}

fn is_one(v: &f64) -> bool {
    *v == 1.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
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

/// The shape of a mask component. In a recipe file its name is the
/// component's `shape` key, beside the component's own settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "shape", rename_all = "lowercase")]
pub enum Shape {
    /// Brush strokes, built up (and erased) on their own.
    Brush {
        #[serde(rename = "stroke", default, skip_serializing_if = "Vec::is_empty")]
        strokes: Vec<Stroke>,
    },
    Linear {
        from: [f64; 2],
        to: [f64; 2],
    },
    /// A circle. Hard-edged by default, so the feather decides how soft it
    /// is (Phil, 2026-11-22: "the feather should be what makes it fade"); a
    /// lower `hardness` fades it toward the centre, like a brush.
    Radial {
        /// Centre.
        from: [f64; 2],
        /// A point on the edge.
        to: [f64; 2],
        #[serde(default = "hard")]
        hardness: f64,
    },
    /// A rectangle, turned clockwise by `angle` degrees about its centre.
    Rect {
        /// Centre, as fractions of the width and height.
        center: [f64; 2],
        /// Width and height before turning, as shares of the longer side
        /// (so a square stays square on any picture).
        size: [f64; 2],
        #[serde(default, skip_serializing_if = "is_zero")]
        angle: f64,
    },
    /// A closed outline drawn with the pen; the last point joins the first.
    /// Each point is `[x, y]`, a corner, or `[x, y, hx, hy]`, a smooth point
    /// whose handle sits at `hx, hy`. The handle bends the side after the
    /// point, and its mirror image the side before, so the outline passes
    /// through smoothly.
    Path {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        points: Vec<Vec<f64>>,
    },
    Brightness {
        /// Brightness range that is fully on, 0..1.
        low: f64,
        high: f64,
        /// Width of the fade on either side of the range.
        #[serde(default)]
        soft: f64,
    },
}

/// One component of a mask: a shape and how it joins the rest. In a recipe
/// file:
///
/// ```toml
/// [[step.mask.op]]
/// shape = "brush"
/// mode = "add"
/// opacity = 0.8        # optional, default 1
/// feather = 0.01       # optional, share of the longer side
/// off = true           # optional: kept but not used
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
pub struct MaskOp {
    // The shape's own keys sit beside the common ones (`flatten`), so a
    // recipe reads `shape = "radial"`, `from`, `to`, `mode` at one level.
    #[serde(flatten)]
    pub shape: Shape,
    #[serde(default)]
    pub mode: Mode,
    /// How strongly the component counts, 0..1: at 0.5 it does half of
    /// what it would.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub opacity: f64,
    /// Blur of this component's edges, as a share of the longer side.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub feather: f64,
    /// Switched off: kept, but left out of the mask as if it were not
    /// there (D-75, recipe version 9). The rest combine without it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub off: bool,
}

impl MaskOp {
    /// A component at full opacity with no feather.
    pub fn new(shape: Shape, mode: Mode) -> MaskOp {
        MaskOp {
            shape,
            mode,
            opacity: 1.0,
            feather: 0.0,
            off: false,
        }
    }
}

/// A step's mask. Unknown keys are refused, so a recipe from before
/// feather moved onto each component fails loudly instead of rendering
/// without its feather.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mask {
    /// Swap shown and hidden: paint where the glitch should *not* be.
    #[serde(default, skip_serializing_if = "is_false")]
    pub invert: bool,
    /// Switched off: the mask is kept but not used, so the step shows
    /// everywhere until it is switched back on (D-73, recipe version 9).
    #[serde(default, skip_serializing_if = "is_false")]
    pub off: bool,
    #[serde(rename = "op", default, skip_serializing_if = "Vec::is_empty")]
    pub ops: Vec<MaskOp>,
}

/// How a mask drawn on the whole photo lands on a cropped canvas (D-52).
///
/// Masks are kept as shares of the uncropped photo, so they stay on the
/// same part of it whatever the crop. Before rendering, positions move by
/// the crop's corner and scale by the crop's size, and lengths (shares of
/// the longer side) scale by how much longer the photo's longer side is
/// than the crop's. In the photo's own pixels that is just a shift, so
/// circles stay round and nothing is stretched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// The crop's corner, as shares of the photo.
    pub x: f64,
    pub y: f64,
    /// Photo width over crop width, and the same for heights.
    pub sx: f64,
    pub sy: f64,
    /// The photo's longer side over the crop's longer side.
    pub sl: f64,
}

impl Frame {
    /// The frame for a crop of `(x, y, w, h)` pixels out of a `pw` x `ph`
    /// photo.
    pub fn new(crop: (usize, usize, usize, usize), pw: usize, ph: usize) -> Frame {
        let (x, y, w, h) = crop;
        let (pw, ph) = (pw.max(1) as f64, ph.max(1) as f64);
        Frame {
            x: x as f64 / pw,
            y: y as f64 / ph,
            sx: pw / w.max(1) as f64,
            sy: ph / h.max(1) as f64,
            sl: pw.max(ph) / w.max(h).max(1) as f64,
        }
    }

    fn at(&self, x: f64, y: f64) -> (f64, f64) {
        ((x - self.x) * self.sx, (y - self.y) * self.sy)
    }
}

/// Limits that keep a hostile or broken recipe from eating the machine.
const MAX_OPS: usize = 2000;
const MAX_POINTS: usize = 400_000;
/// Largest feather, as a share of the longer side.
pub const MAX_FEATHER: f64 = 0.25;
/// Largest rectangle side, as a share of the longer side: room to reach
/// past every corner even when turned.
const MAX_SIDE: f64 = 4.0;

impl Mask {
    /// Check every value; a bad mask fails before any work.
    pub fn validate(&self) -> Result<()> {
        let bad = |msg: String| Err(Error::Recipe(format!("mask: {msg}")));
        let unit = |v: f64| v.is_finite() && (0.0..=1.0).contains(&v);
        // Positions may sit a little outside the picture (a stroke that
        // runs off the edge), but not absurdly far.
        let pos = |v: f64| v.is_finite() && (-4.0..=5.0).contains(&v);
        if self.ops.len() > MAX_OPS {
            return bad(format!("{} operations (at most {MAX_OPS})", self.ops.len()));
        }
        let mut points = 0;
        for op in &self.ops {
            if !unit(op.opacity) {
                return bad(format!("opacity {} must be 0..1", op.opacity));
            }
            if !(op.feather.is_finite() && (0.0..=MAX_FEATHER).contains(&op.feather)) {
                return bad(format!("feather {} must be 0..{MAX_FEATHER}", op.feather));
            }
            match &op.shape {
                Shape::Brush { strokes } => {
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
                Shape::Linear { from, to } => {
                    if !from.iter().chain(to).all(|&v| pos(v)) {
                        return bad("gradient ends must be near the picture".into());
                    }
                }
                Shape::Radial { from, to, hardness } => {
                    if !from.iter().chain(to).all(|&v| pos(v)) {
                        return bad("circle center and edge must be near the picture".into());
                    }
                    if !unit(*hardness) {
                        return bad(format!("circle hardness {hardness} must be 0..1"));
                    }
                }
                Shape::Rect {
                    center,
                    size,
                    angle,
                } => {
                    if !center.iter().all(|&v| pos(v)) {
                        return bad("rectangle center must be near the picture".into());
                    }
                    if !size
                        .iter()
                        .all(|&v| v.is_finite() && (0.0..=MAX_SIDE).contains(&v))
                    {
                        return bad(format!("rectangle size must be 0..{MAX_SIDE}"));
                    }
                    if !(angle.is_finite() && angle.abs() <= 360.0) {
                        return bad(format!("rectangle angle {angle} must be -360..360"));
                    }
                }
                Shape::Path { points: p } => {
                    for q in p {
                        if !(q.len() == 2 || q.len() == 4) || !q.iter().all(|&v| pos(v)) {
                            return bad(
                                "path points must be [x, y] or [x, y, hx, hy] near the picture"
                                    .into(),
                            );
                        }
                    }
                    points += p.len();
                }
                Shape::Brightness { low, high, soft } => {
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

    /// The mask as drawn, with every switch on: the whole mask and each
    /// component. The GUI's overlay shows this, so a component switched off
    /// can still be seen while it is edited (D-74); the switches change
    /// the render only.
    pub fn as_drawn(&self) -> Mask {
        Mask {
            off: false,
            ops: self
                .ops
                .iter()
                .map(|op| MaskOp {
                    off: false,
                    ..op.clone()
                })
                .collect(),
            ..self.clone()
        }
    }

    /// This mask moved onto a cropped canvas (see [`Frame`]). The result is
    /// only for rendering: its values may lie beyond the limits `validate`
    /// checks, which apply to the mask as stored.
    pub fn reframed(&self, f: &Frame) -> Mask {
        let pt = |p: &[f64; 2]| {
            let (x, y) = f.at(p[0], p[1]);
            [x, y]
        };
        let ops = self
            .ops
            .iter()
            .map(|op| {
                let shape = match &op.shape {
                    Shape::Brush { strokes } => Shape::Brush {
                        strokes: strokes
                            .iter()
                            .map(|s| Stroke {
                                size: s.size * f.sl,
                                points: s
                                    .points
                                    .chunks_exact(2)
                                    .flat_map(|q| {
                                        let (x, y) = f.at(q[0], q[1]);
                                        [x, y]
                                    })
                                    .collect(),
                                ..s.clone()
                            })
                            .collect(),
                    },
                    Shape::Linear { from, to } => Shape::Linear {
                        from: pt(from),
                        to: pt(to),
                    },
                    Shape::Radial { from, to, hardness } => Shape::Radial {
                        from: pt(from),
                        to: pt(to),
                        hardness: *hardness,
                    },
                    Shape::Rect {
                        center,
                        size,
                        angle,
                    } => Shape::Rect {
                        center: pt(center),
                        size: [size[0] * f.sl, size[1] * f.sl],
                        angle: *angle,
                    },
                    Shape::Path { points } => Shape::Path {
                        points: points
                            .iter()
                            .map(|q| {
                                q.chunks_exact(2)
                                    .flat_map(|p| {
                                        let (x, y) = f.at(p[0], p[1]);
                                        [x, y]
                                    })
                                    .collect()
                            })
                            .collect(),
                    },
                    Shape::Brightness { .. } => op.shape.clone(),
                };
                MaskOp {
                    shape,
                    feather: op.feather * f.sl,
                    ..op.clone()
                }
            })
            .collect();
        Mask {
            invert: self.invert,
            off: self.off,
            ops,
        }
    }

    /// The mask for a `w` x `h` raster whose bytes (R, G, B per pixel) are
    /// `input`, the picture as it arrives at the step. One value per pixel,
    /// 0..1, in raster order.
    pub fn render(&self, w: usize, h: usize, input: &[u8], cancel: Cancel<'_>) -> Result<Vec<f32>> {
        let mut m = vec![0f32; w * h];
        // Lengths are shares of the longer side, in this raster's pixels.
        let long = w.max(h) as f64;
        // Each component's coverage is drawn on its own (so its feather
        // blurs only its edges), then blended into the mask. One buffer,
        // reused.
        let mut c = vec![0f32; w * h];
        // A component switched off counts as absent.
        for op in self.ops.iter().filter(|op| !op.off) {
            if cancel() {
                return Err(Error::Cancelled);
            }
            c.fill(0.0);
            coverage(&op.shape, &mut c, w, h, long, input);
            // Three box blurs in a row approximate a Gaussian; a box of
            // radius k three times has a standard deviation of about k.
            let k = (op.feather * long).round() as usize;
            if k > 0 {
                for _ in 0..3 {
                    if cancel() {
                        return Err(Error::Cancelled);
                    }
                    blur_rows(&mut c, w, h, k);
                    blur_columns(&mut c, w, h, k);
                }
            }
            blend(&mut m, &c, op.mode, op.opacity);
        }
        if self.invert {
            for v in &mut m {
                *v = 1.0 - *v;
            }
        }
        Ok(m)
    }
}

/// Draw one shape's coverage (0..1 per pixel) into `c`, which starts at 0.
fn coverage(shape: &Shape, c: &mut [f32], w: usize, h: usize, long: f64, input: &[u8]) {
    match shape {
        Shape::Brush { strokes } => brush(c, w, h, long, strokes),
        Shape::Linear { from, to } => {
            let (a, d) = ends(from, to, w, h);
            let len2 = d.0 * d.0 + d.1 * d.1;
            each_pixel(c, w, |px, py| {
                if len2 < 1e-9 {
                    return 1.0;
                }
                let t = ((px - a.0) * d.0 + (py - a.1) * d.1) / len2;
                1.0 - t.clamp(0.0, 1.0)
            });
        }
        Shape::Radial { from, to, hardness } => {
            let (a, d) = ends(from, to, w, h);
            let r = (d.0 * d.0 + d.1 * d.1).sqrt();
            let edge = edge_width(r, *hardness);
            each_pixel(c, w, |px, py| {
                let (dx, dy) = (px - a.0, py - a.1);
                soft_edge(r, (dx * dx + dy * dy).sqrt(), edge)
            });
        }
        Shape::Rect {
            center,
            size,
            angle,
        } => fill::fill(c, w, h, &[rect_corners(center, size, *angle, w, h, long)]),
        Shape::Path { points } => fill::fill(c, w, h, &[path_outline(points, w, h)]),
        Shape::Brightness { low, high, soft } => {
            let (low, high, soft) = (*low, *high, *soft);
            for (i, v) in c.iter_mut().enumerate() {
                let p = &input[i * 3..i * 3 + 3];
                let l =
                    (0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64) / 255.0;
                *v = if soft > 0.0 {
                    let up = ((l - (low - soft)) / soft).clamp(0.0, 1.0);
                    let down = (((high + soft) - l) / soft).clamp(0.0, 1.0);
                    up.min(down)
                } else if low <= l && l <= high {
                    1.0
                } else {
                    0.0
                } as f32;
            }
        }
    }
}

/// A rectangle's corners in pixels, in order around it.
fn rect_corners(
    center: &[f64; 2],
    size: &[f64; 2],
    angle: f64,
    w: usize,
    h: usize,
    long: f64,
) -> Vec<Pt> {
    let (cx, cy) = (center[0] * w as f64, center[1] * h as f64);
    let (hu, hv) = (size[0] * long * 0.5, size[1] * long * 0.5);
    // Unturned is the common case; keep it exact (cos_turns(0) is 1 only
    // to within the last bit).
    let (cos, sin) = if angle == 0.0 {
        (1.0, 0.0)
    } else {
        let t = angle / 360.0;
        (dsp::cos_turns(t), dsp::sin_turns(t))
    };
    // Clockwise on screen, since y points down: the width runs along
    // (cos, sin) and the height along (-sin, cos).
    let corner = |su: f64, sv: f64| {
        (
            cx + su * hu * cos - sv * hv * sin,
            cy + su * hu * sin + sv * hv * cos,
        )
    };
    vec![
        corner(-1.0, -1.0),
        corner(1.0, -1.0),
        corner(1.0, 1.0),
        corner(-1.0, 1.0),
    ]
}

/// A pen path's outline in pixels: straight sides between corners, and
/// cubic curves where a side starts or ends at a smooth point.
fn path_outline(points: &[Vec<f64>], w: usize, h: usize) -> Vec<Pt> {
    let (wf, hf) = (w as f64, h as f64);
    let anchor = |p: &[f64]| (p[0] * wf, p[1] * hf);
    let handle = |p: &[f64]| (p.len() == 4).then(|| (p[2] * wf, p[3] * hf));
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let mut out = vec![anchor(&points[0])];
    for i in 0..n {
        let (p, q) = (&points[i], &points[(i + 1) % n]);
        let (a, d) = (anchor(p), anchor(q));
        match (handle(p), handle(q)) {
            (None, None) => out.push(d),
            (hp, hq) => {
                // The side leaves p along p's handle and arrives at q along
                // the mirror of q's.
                let b = hp.unwrap_or(a);
                let c = hq.map_or(d, |hq| (2.0 * d.0 - hq.0, 2.0 * d.1 - hq.1));
                fill::flatten_cubic(a, b, c, d, &mut out);
            }
        }
    }
    // The last side ended back on the first point.
    out.pop();
    out
}

/// Combine the mask so far with a component's value at one pixel.
fn combine(m: f64, v: f64, mode: Mode) -> f64 {
    match mode {
        Mode::Add => m + v - m * v,
        Mode::Subtract => m * (1.0 - v),
        Mode::Intersect => m * v,
    }
}

/// Blend a component's coverage `c` into the mask `m` with its mode, scaled
/// by its opacity: the mask moves `opacity` of the way from where it was to
/// where the full combination would take it. For add and subtract that is
/// the same as scaling the coverage; for intersect it lets an opacity below
/// 1 limit only partly, instead of darkening everything.
fn blend(m: &mut [f32], c: &[f32], mode: Mode, opacity: f64) {
    for (mv, &cv) in m.iter_mut().zip(c) {
        let (a, v) = (*mv as f64, cv as f64);
        let full = combine(a, v, mode);
        // At full opacity, exactly the combination (no rounding detour).
        let out = if opacity == 1.0 {
            full
        } else {
            a + (full - a) * opacity
        };
        *mv = out as f32;
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

/// Set every pixel to `f(x, y)` (pixel centres).
fn each_pixel(c: &mut [f32], w: usize, f: impl Fn(f64, f64) -> f64) {
    for (i, v) in c.iter_mut().enumerate() {
        let (x, y) = ((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
        *v = f(x, y) as f32;
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

/// A brush's coverage: its strokes in order, each painted (or erased) on
/// the brush's own layer, touching only the pixels it can reach.
fn brush(c: &mut [f32], w: usize, h: usize, long: f64, strokes: &[Stroke]) {
    for s in strokes {
        let (pts, radius) = stroke_geometry(s, w, h, long);
        let Some(r) = stroke_rect(&pts, radius, w, h) else {
            continue;
        };
        let one = stroke_coverage(&pts, radius, s.hardness, r);
        let how = if s.erase { Mode::Subtract } else { Mode::Add };
        for y in r.y0..r.y1 {
            for x in r.x0..r.x1 {
                let v = one[(y - r.y0) * r.width() + (x - r.x0)] as f64;
                let i = y * w + x;
                c[i] = combine(c[i] as f64, v, how) as f32;
            }
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

    fn render(mask: &Mask, w: usize, h: usize) -> Vec<f32> {
        mask.render(w, h, &grey(w, h, 0), NEVER).unwrap()
    }

    fn masked(ops: Vec<MaskOp>) -> Mask {
        Mask {
            ops,
            ..Mask::default()
        }
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
        MaskOp::new(
            Shape::Brush {
                strokes: vec![dab(false, x, y, size)],
            },
            mode,
        )
    }

    fn circle(mode: Mode, from: [f64; 2], to: [f64; 2]) -> MaskOp {
        MaskOp::new(
            Shape::Radial {
                from,
                to,
                hardness: 1.0,
            },
            mode,
        )
    }

    /// On (1) left of x = 0.5, off right of it: a linear gradient squeezed
    /// to a step.
    fn left_half() -> MaskOp {
        MaskOp::new(
            Shape::Linear {
                from: [0.5, 0.0],
                to: [0.5001, 0.0],
            },
            Mode::Add,
        )
    }

    #[test]
    fn empty_mask_hides_everything_and_invert_shows_it() {
        assert!(render(&Mask::default(), 4, 4).iter().all(|&v| v == 0.0));
        let inv = Mask {
            invert: true,
            ..Mask::default()
        };
        assert!(render(&inv, 4, 4).iter().all(|&v| v == 1.0));
    }

    #[test]
    fn a_stroke_covers_its_path_and_nothing_else() {
        // A horizontal stroke across the middle of a 40 x 20 raster.
        let mask = masked(vec![MaskOp::new(
            Shape::Brush {
                strokes: vec![Stroke {
                    erase: false,
                    size: 0.1, // 4 px wide
                    hardness: 1.0,
                    points: vec![0.1, 0.5, 0.9, 0.5],
                }],
            },
            Mode::Add,
        )]);
        let m = render(&mask, 40, 20);
        assert_eq!(at(&m, 40, 20, 10), 1.0);
        assert_eq!(at(&m, 40, 20, 2), 0.0);
        assert_eq!(at(&m, 40, 1, 10), 0.0);
    }

    #[test]
    fn the_eraser_takes_back_its_own_brush_only() {
        // A radial underneath, then a brush whose eraser crosses both.
        let mask = masked(vec![
            circle(Mode::Add, [0.25, 0.5], [0.4, 0.5]),
            MaskOp::new(
                Shape::Brush {
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
                Mode::Add,
            ),
        ]);
        let m = render(&mask, 40, 20);
        assert_eq!(at(&m, 40, 30, 10), 0.0, "the brush's own paint is erased");
        assert_eq!(at(&m, 40, 30, 5), 1.0, "the rest of the brush stays");
        assert_eq!(at(&m, 40, 10, 10), 1.0, "the radial is untouched");
    }

    #[test]
    fn a_subtracting_brush_cuts_into_everything() {
        let mask = masked(vec![
            circle(Mode::Add, [0.5, 0.5], [1.0, 0.5]),
            dot(Mode::Subtract, 0.5, 0.5, 0.2),
        ]);
        let m = render(&mask, 20, 20);
        assert_eq!(at(&m, 20, 10, 10), 0.0, "cut out");
        assert_eq!(at(&m, 20, 10, 6), 1.0, "radial around it");
    }

    #[test]
    fn an_empty_brush_changes_nothing_unless_it_intersects() {
        let base = circle(Mode::Add, [0.5, 0.5], [1.0, 0.5]);
        let with = |mode| {
            masked(vec![
                base.clone(),
                MaskOp::new(Shape::Brush { strokes: vec![] }, mode),
            ])
        };
        let plain = render(&masked(vec![base.clone()]), 10, 10);
        assert_eq!(render(&with(Mode::Add), 10, 10), plain);
        assert_eq!(render(&with(Mode::Subtract), 10, 10), plain);
        assert!(
            render(&with(Mode::Intersect), 10, 10)
                .iter()
                .all(|&v| v == 0.0)
        );
    }

    #[test]
    fn linear_gradient_fades_from_start_to_end() {
        let mask = masked(vec![MaskOp::new(
            Shape::Linear {
                from: [0.0, 0.5],
                to: [1.0, 0.5],
            },
            Mode::Add,
        )]);
        let m = render(&mask, 10, 2);
        let row: Vec<f32> = (0..10).map(|x| at(&m, 10, x, 0)).collect();
        assert!(row.windows(2).all(|p| p[0] > p[1]), "{row:?}");
        assert!((row[0] - 0.95).abs() < 1e-6 && (row[9] - 0.05).abs() < 1e-6);
    }

    #[test]
    fn radial_is_a_hard_circle_unless_softened() {
        let ring = |hardness: f64| {
            masked(vec![MaskOp::new(
                Shape::Radial {
                    from: [0.5, 0.5],
                    to: [0.75, 0.5], // radius 10 px on a 40 px square
                    hardness,
                },
                Mode::Add,
            )])
        };
        let hard = render(&ring(1.0), 40, 40);
        // Full right up to the edge, nothing past it: no fade of its own.
        assert_eq!(at(&hard, 40, 20, 20), 1.0);
        assert_eq!(at(&hard, 40, 28, 20), 1.0);
        assert_eq!(at(&hard, 40, 31, 20), 0.0);
        assert_eq!(at(&hard, 40, 0, 0), 0.0);
        // A lower hardness fades toward the centre, like a soft brush.
        let soft = render(&ring(0.0), 40, 40);
        let v = at(&soft, 40, 26, 20);
        assert!(v > 0.05 && v < 0.6, "{v}");
        // A recipe that leaves hardness (and mode, opacity, feather) out
        // gets the hard circle, added at full strength.
        let op: MaskOp =
            toml::from_str("shape = \"radial\"\nfrom = [0.5, 0.5]\nto = [0.6, 0.5]").unwrap();
        assert_eq!(op, circle(Mode::Add, [0.5, 0.5], [0.6, 0.5]));
    }

    fn rect(center: [f64; 2], size: [f64; 2], angle: f64) -> Mask {
        masked(vec![MaskOp::new(
            Shape::Rect {
                center,
                size,
                angle,
            },
            Mode::Add,
        )])
    }

    fn sum(m: &[f32]) -> f64 {
        m.iter().map(|&v| v as f64).sum()
    }

    #[test]
    fn a_rectangle_is_crisp_and_turns_about_its_centre() {
        // 40 x 20 raster: sizes are shares of the longer side (40 px), so
        // [0.5, 0.25] is 20 x 10 px, centred: x 10..30, y 5..15.
        let m = render(&rect([0.5, 0.5], [0.5, 0.25], 0.0), 40, 20);
        assert_eq!(at(&m, 40, 10, 5), 1.0);
        assert_eq!(at(&m, 40, 29, 14), 1.0);
        assert_eq!(at(&m, 40, 9, 10), 0.0);
        assert_eq!(at(&m, 40, 20, 15), 0.0);
        assert_eq!(sum(&m), 200.0);
        // Turned a quarter: 10 wide, 20 tall, still centred.
        let m = render(&rect([0.5, 0.5], [0.5, 0.25], 90.0), 40, 40);
        assert!(at(&m, 40, 20, 11) > 0.999 && at(&m, 40, 20, 28) > 0.999);
        assert!(at(&m, 40, 12, 20) < 0.001, "the old width is gone");
        // Any turn keeps the area, anti-aliased rather than jagged.
        let m = render(&rect([0.5, 0.5], [0.5, 0.25], 30.0), 40, 40);
        assert!((sum(&m) - 200.0).abs() < 1e-3, "{}", sum(&m));
        assert!(
            m.iter().any(|&v| v > 0.1 && v < 0.9),
            "edges are anti-aliased"
        );
        // Turning by 180 changes nothing visible (to within rounding).
        let a = render(&rect([0.4, 0.6], [0.3, 0.2], 0.0), 40, 40);
        let b = render(&rect([0.4, 0.6], [0.3, 0.2], 180.0), 40, 40);
        assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-6));
    }

    #[test]
    fn a_path_fills_its_outline_and_curves_at_smooth_points() {
        let path =
            |points: Vec<Vec<f64>>| masked(vec![MaskOp::new(Shape::Path { points }, Mode::Add)]);
        // A right triangle with corners: half the square under it.
        let tri = path(vec![vec![0.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]]);
        let m = render(&tri, 20, 20);
        assert!((sum(&m) - 200.0).abs() < 1e-3);
        assert_eq!(at(&m, 20, 2, 2), 1.0);
        assert_eq!(at(&m, 20, 17, 17), 0.0);
        // A diamond of corners, then the same points made smooth with
        // handles along the square's sides: the curves bulge out toward
        // the square, so more is covered, but never past it.
        let corners = vec![
            vec![0.5, 0.1],
            vec![0.9, 0.5],
            vec![0.5, 0.9],
            vec![0.1, 0.5],
        ];
        let smooth = vec![
            vec![0.5, 0.1, 0.7, 0.1],
            vec![0.9, 0.5, 0.9, 0.7],
            vec![0.5, 0.9, 0.3, 0.9],
            vec![0.1, 0.5, 0.1, 0.3],
        ];
        let (d, r) = (
            render(&path(corners), 40, 40),
            render(&path(smooth), 40, 40),
        );
        assert!((sum(&d) - 0.5 * 32.0 * 32.0).abs() < 1e-3);
        assert!(sum(&r) > sum(&d) + 100.0 && sum(&r) < 32.0 * 32.0);
        assert_eq!(at(&r, 40, 0, 0), 0.0);
        // Too few points to enclose anything: nothing, but no error.
        assert!(
            render(&path(vec![vec![0.2, 0.2], vec![0.8, 0.8]]), 10, 10)
                .iter()
                .all(|&v| v == 0.0)
        );
        assert!(render(&path(vec![]), 10, 10).iter().all(|&v| v == 0.0));
    }

    #[test]
    fn rectangles_and_paths_read_from_a_recipe() {
        let op: MaskOp = toml::from_str(
            "shape = \"rect\"\ncenter = [0.5, 0.5]\nsize = [0.2, 0.1]\nmode = \"subtract\"",
        )
        .unwrap();
        assert_eq!(
            op,
            MaskOp::new(
                Shape::Rect {
                    center: [0.5, 0.5],
                    size: [0.2, 0.1],
                    angle: 0.0
                },
                Mode::Subtract
            )
        );
        let op: MaskOp = toml::from_str(
            "shape = \"path\"\npoints = [[0.1, 0.1], [0.9, 0.2, 0.95, 0.4], [0.3, 0.8]]",
        )
        .unwrap();
        let Shape::Path { points } = &op.shape else {
            panic!("{op:?}")
        };
        assert_eq!(points[1], vec![0.9, 0.2, 0.95, 0.4]);
        // And back out: an unturned rectangle leaves its angle out.
        let text = toml::to_string(&rect([0.5, 0.5], [0.2, 0.1], 0.0).ops[0]).unwrap();
        assert!(!text.contains("angle"), "{text}");
    }

    #[test]
    fn brightness_selects_by_level() {
        let mut input = grey(2, 1, 30);
        input[3..6].copy_from_slice(&[230, 230, 230]);
        let mask = masked(vec![MaskOp::new(
            Shape::Brightness {
                low: 0.5,
                high: 1.0,
                soft: 0.0,
            },
            Mode::Add,
        )]);
        assert_eq!(mask.render(2, 1, &input, NEVER).unwrap(), vec![0.0, 1.0]);
    }

    #[test]
    fn intersect_keeps_only_the_overlap() {
        let mask = masked(vec![
            MaskOp::new(
                Shape::Linear {
                    from: [0.0, 0.0],
                    to: [0.0, 0.0],
                },
                Mode::Add,
            ),
            dot(Mode::Intersect, 0.25, 0.5, 0.3),
        ]);
        let m = render(&mask, 20, 10);
        assert_eq!(at(&m, 20, 5, 5), 1.0);
        assert_eq!(at(&m, 20, 15, 5), 0.0);
    }

    #[test]
    fn feather_softens_its_own_component_only() {
        let (w, h) = (100, 40);
        let hard = render(&masked(vec![left_half()]), w, h);
        assert_eq!((at(&hard, w, 48, 20), at(&hard, w, 52, 20)), (1.0, 0.0));

        // The same edge feathered, next to a hard circle on the right.
        let soft_half = MaskOp {
            feather: 0.05,
            ..left_half()
        };
        let m = render(
            &masked(vec![soft_half, circle(Mode::Add, [0.85, 0.5], [0.95, 0.5])]),
            w,
            h,
        );
        // The feathered edge ramps; its middle stays on.
        let (l, r) = (at(&m, w, 48, 20), at(&m, w, 52, 20));
        assert!(l < 1.0 && l > 0.5 && r > 0.0 && r < 0.5, "{l} {r}");
        assert!(at(&m, w, 5, 20) > 0.999);
        // The circle (radius 10 px around x = 85) keeps its hard edge.
        assert_eq!(at(&m, w, 85, 20), 1.0);
        assert_eq!(at(&m, w, 93, 20), 1.0);
        assert!(at(&m, w, 96, 20) < 0.01);
    }

    #[test]
    fn opacity_scales_what_a_component_does() {
        let half = |op: MaskOp| MaskOp { opacity: 0.5, ..op };
        // Added at half strength.
        let m = render(&masked(vec![half(left_half())]), 10, 2);
        assert_eq!((at(&m, 10, 2, 0), at(&m, 10, 8, 0)), (0.5, 0.0));
        // Subtracted at half strength: halves what was there. (A dot of
        // radius 3 px, so the pixel tested is fully inside it.)
        let m = render(
            &masked(vec![left_half(), half(dot(Mode::Subtract, 0.2, 0.5, 0.3))]),
            20,
            20,
        );
        assert_eq!(at(&m, 20, 4, 10), 0.5);
        // Intersected at half strength: outside the limit keeps half, and
        // nothing darkens inside it.
        let all_on = MaskOp::new(
            Shape::Linear {
                from: [0.0, 0.0],
                to: [0.0, 0.0],
            },
            Mode::Add,
        );
        let mut limit = half(left_half());
        limit.mode = Mode::Intersect;
        let m = render(&masked(vec![all_on, limit]), 10, 2);
        assert_eq!((at(&m, 10, 2, 0), at(&m, 10, 8, 0)), (1.0, 0.5));
        // At zero a component does nothing at all.
        let m = render(
            &masked(vec![MaskOp {
                opacity: 0.0,
                ..left_half()
            }]),
            10,
            2,
        );
        assert!(m.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn a_component_switched_off_counts_as_absent() {
        // A circle cut out of the left half; with the cut switched off the
        // half is whole again, and with the half off nothing is left.
        let cut = circle(Mode::Subtract, [0.25, 0.5], [0.35, 0.5]);
        let whole = render(&masked(vec![left_half()]), 40, 20);
        let off = MaskOp {
            off: true,
            ..cut.clone()
        };
        assert_eq!(render(&masked(vec![left_half(), off]), 40, 20), whole);
        assert!(
            at(
                &render(&masked(vec![left_half(), cut.clone()]), 40, 20),
                40,
                10,
                10
            ) < 1.0
        );
        let half_off = MaskOp {
            off: true,
            ..left_half()
        };
        assert!(
            render(&masked(vec![half_off, cut]), 40, 20)
                .iter()
                .all(|&v| v == 0.0)
        );
    }

    #[test]
    fn a_preview_sized_mask_matches_the_full_one() {
        let mask = masked(vec![
            MaskOp {
                feather: 0.02,
                ..MaskOp::new(
                    Shape::Brush {
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
                    Mode::Add,
                )
            },
            MaskOp {
                opacity: 0.7,
                feather: 0.01,
                ..circle(Mode::Subtract, [0.5, 0.6], [0.6, 0.6])
            },
        ]);
        let full = render(&mask, 400, 300);
        let small = render(&mask, 100, 75);
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
        let bad = |op: MaskOp| masked(vec![op]).validate().is_err();
        assert!(bad(dot(Mode::Add, 0.5, 0.5, 0.0)));
        assert!(bad(MaskOp::new(
            Shape::Brush {
                strokes: vec![Stroke {
                    erase: false,
                    size: 0.1,
                    hardness: 0.5,
                    points: vec![0.5],
                }],
            },
            Mode::Add,
        )));
        assert!(bad(MaskOp::new(
            Shape::Brightness {
                low: 0.8,
                high: 0.2,
                soft: 0.0,
            },
            Mode::Add,
        )));
        assert!(bad(dot(Mode::Add, f64::NAN, 0.5, 0.1)));
        assert!(bad(MaskOp {
            feather: 0.9,
            ..dot(Mode::Add, 0.5, 0.5, 0.1)
        }));
        assert!(bad(MaskOp {
            opacity: 1.5,
            ..dot(Mode::Add, 0.5, 0.5, 0.1)
        }));
        assert!(!bad(dot(Mode::Add, 0.5, 0.5, 0.1)));
        let shape = |shape| MaskOp::new(shape, Mode::Add);
        let rect = |size: [f64; 2], angle| {
            shape(Shape::Rect {
                center: [0.5, 0.5],
                size,
                angle,
            })
        };
        assert!(bad(rect([-0.1, 0.2], 0.0)));
        assert!(bad(rect([0.1, 0.2], f64::INFINITY)));
        assert!(bad(rect([0.1, 0.2], 400.0)));
        assert!(!bad(rect([0.1, 0.2], -45.0)));
        let path = |points: Vec<Vec<f64>>| shape(Shape::Path { points });
        assert!(bad(path(vec![vec![0.1, 0.1, 0.2]])));
        assert!(bad(path(vec![vec![0.1, f64::NAN]])));
        assert!(!bad(path(vec![vec![0.1, 0.1], vec![0.5, 0.5, 0.6, 0.5]])));
    }

    #[test]
    fn a_mask_wide_feather_is_refused() {
        // Feather moved onto each component; an old recipe must not
        // quietly lose it.
        let err = toml::from_str::<Mask>("feather = 0.01\n").unwrap_err();
        assert!(err.to_string().contains("feather"), "{err}");
    }
}
