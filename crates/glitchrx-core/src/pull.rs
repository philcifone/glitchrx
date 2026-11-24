//! `pull`: drag pixels around by hand, like Liquify's forward warp (SPEC 8.2,
//! D-64).
//!
//! Unlike every other effect, a pull step is not a formula over the bytes:
//! it is a list of strokes drawn on the photo, kept in the recipe as
//! `[[step.pull]]` tables. Because it is still a step, it switches on and
//! off, takes opacity and a mask, and moves in the stack like any other.
//!
//! **The displacement map.** The strokes are turned into a map at a fixed
//! size ([`MAP_SIDE`] points on the canvas's longer side) that says, for
//! every point of the output, how far away in the input to read. The map's
//! size depends only on the canvas at full resolution, never on the raster
//! being rendered, so a preview and an export read through the same map and
//! agree more closely than most effects do.
//!
//! **Building it.** Each stroke moves along its points in small steps, one
//! every quarter of the brush radius. A step from `a` to `b` pulls what was
//! at `p - w (b - a)` to `p`, for every map point `p` under the brush,
//! where `w` is the strength times the falloff `(1 - t^2)^2` (`t` the
//! distance from `b` over the radius). The new value is read from the map
//! as it stood before the step, through bilinear interpolation, so later
//! steps carry earlier ones along rather than tearing them: this is the
//! composition that keeps a long drag smooth.
//!
//! **Rendering.** Every pixel is then read once from the step's input,
//! through the map (itself interpolated, so its 1024-point grid never shows
//! as blocks): the nearest pixel (`pixels`, hard-edged, the default, which
//! suits the raw-pixel look) or a bilinear blend (`smooth`, like Liquify).
//! Reads past an edge take the edge pixel.
//!
//! **Pins** (D-67). A pin is a point picked up and put down elsewhere,
//! `[[step.pin]]` with `from`, `to` and `size`. It is built exactly as a
//! straight stroke from `from` to `to` at full strength, so the point under
//! it lands where it was put and the picture around it stretches smoothly
//! rather than folding, however far it goes. Pins run after the strokes,
//! in order. Unlike a stroke a pin is meant to be moved again: the GUI
//! changes its `to`, and the map is built afresh.
//!
//! **Speed** (8c). Building the map is most of the cost (about 8 ms a
//! stroke on a laptop), and while a stroke is drawn the step is rendered
//! every frame with all the earlier strokes too. So maps are kept at a few
//! checkpoints ([`Checkpoints`]): before the last stroke (the one being
//! drawn), after the strokes (where pins start), where this build first
//! differs from the last (the pin being moved), and whole. A build starts from the longest checkpoint its strokes begin
//! with. A map built from a checkpoint is the very same map, since the
//! strokes run in the same order on the same values.
//!
//! Determinism (D-25): basic f64 operations only (and `sqrt`, which IEEE
//! specifies exactly), no randomness, so no seed matters.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::effects::{Effect, EffectInfo, Group, PlacementDefaults, Window};
use crate::error::{Error, Result};
use crate::mask::Frame;
use crate::params::{Kind, ParamSpec, Params, Units};
use crate::rng::Rng;

/// Points of the displacement map along the canvas's longer side.
pub const MAP_SIDE: usize = 1024;

/// Limits that keep a hostile or broken recipe from eating the machine
/// (the same scale as a mask's).
const MAX_STROKES: usize = 5000;
const MAX_POINTS: usize = 400_000;
const MAX_PINS: usize = 2000;
/// Largest brush, as a share of the longer side.
pub const MAX_SIZE: f64 = 1.0;

pub static INFO: EffectInfo = EffectInfo {
    id: "pull",
    group: Group::Pixel,
    version: 1,
    upgrades: &[],
    audacity: "none (drawn by hand)",
    summary: "Drag pixels around by hand: strokes drawn on the photo pull what is under the brush along with them.",
    details: "A step made of strokes rather than settings, like Photoshop's Liquify forward \
              warp: each stroke drags the pixels under the brush along its path, strongest \
              at the centre. Strokes are drawn in the GUI (or written as `[[step.pull]]` \
              tables in a recipe: `size` as a share of the photo's longer side, `strength` \
              0 to 1, and `points` as x, y shares of the photo); added on the command line, \
              the step has none and changes nothing. `sampling=pixels` keeps hard pixel \
              edges; `smooth` blends neighbours, as Liquify does. Placement (`region`, \
              `hits`, `len`) does not apply: the strokes are the placement. Opacity and a \
              mask do, so a pull can be taken back from part of the picture.",
    params: &[ParamSpec {
        name: "sampling",
        kind: Kind::Choice(&["pixels", "smooth"]),
        default: "pixels",
        help: "pixels keeps hard pixel edges; smooth blends neighbours",
    }],
    placement: PlacementDefaults {
        region: "0%:100%",
        hits: "0",
        len: "100%",
    },
    random_within: false,
    exact_preview: false,
    strokes: true,
    example: "glitchrx --recipe pulled.toml photo.jpg",
    build,
};

/// The registry wants a constructor. A pull never runs window by window:
/// `recipe::Step::prepare` builds a [`Pull`] with the step's strokes
/// instead, and the recipe runs that on the whole raster.
fn build(_: &Params) -> Result<Box<dyn Effect>> {
    Ok(Box::new(Windowless))
}

struct Windowless;

impl Effect for Windowless {
    fn run(&self, _: &mut Window<'_>, _: &mut Rng) -> Result<()> {
        Err(Error::Recipe(
            "pull runs on the whole picture through its strokes, not in windows".into(),
        ))
    }
}

fn default_strength() -> f64 {
    0.6
}

/// One stroke, as drawn.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// Brush diameter, as a share of the photo's longer side.
    pub size: f64,
    /// 0 to 1: how far the pixels at the centre follow the finger.
    #[serde(default = "default_strength")]
    pub strength: f64,
    /// x, y pairs, as shares of the photo's width and height, in the order
    /// drawn.
    pub points: Vec<f64>,
}

impl Stroke {
    /// The same stroke on a crop's canvas (see [`Frame`]): points move with
    /// the crop's corner and scale, the size with its longer side.
    fn reframed(&self, f: &Frame) -> Stroke {
        Stroke {
            size: self.size * f.sl,
            strength: self.strength,
            points: self
                .points
                .chunks_exact(2)
                .flat_map(|q| [(q[0] - f.x) * f.sx, (q[1] - f.y) * f.sy])
                .collect(),
        }
    }
}

/// One pin: the point at `from` moved to `to` (x, y shares of the photo),
/// dragging the picture within `size` of it along.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pin {
    /// Brush diameter, as a share of the photo's longer side.
    pub size: f64,
    pub from: [f64; 2],
    pub to: [f64; 2],
}

impl Pin {
    /// The stroke a pin is built as: a straight drag at full strength.
    fn as_stroke(&self) -> Stroke {
        Stroke {
            size: self.size,
            strength: 1.0,
            points: vec![self.from[0], self.from[1], self.to[0], self.to[1]],
        }
    }
}

/// Strokes and pins may run a little off the picture, but not absurdly far.
fn on_canvas(v: f64) -> bool {
    v.is_finite() && (-4.0..=5.0).contains(&v)
}

fn size_ok(size: f64) -> bool {
    size.is_finite() && size > 0.0 && size <= MAX_SIZE
}

/// Check a step's pins; bad ones fail before any work.
pub fn validate_pins(pins: &[Pin]) -> Result<()> {
    let bad = |msg: String| Err(Error::Recipe(format!("pull: {msg}")));
    if pins.len() > MAX_PINS {
        return bad(format!("{} pins (at most {MAX_PINS})", pins.len()));
    }
    for p in pins {
        if !size_ok(p.size) {
            return bad(format!(
                "pin size {} must be above 0 and at most {MAX_SIZE}",
                p.size
            ));
        }
        if !p.from.iter().chain(&p.to).all(|&v| on_canvas(v)) {
            return bad("a pin lies far outside the picture".into());
        }
    }
    Ok(())
}

/// Check a step's strokes; bad ones fail before any work.
pub fn validate(strokes: &[Stroke]) -> Result<()> {
    let bad = |msg: String| Err(Error::Recipe(format!("pull: {msg}")));
    let pos = on_canvas;
    if strokes.len() > MAX_STROKES {
        return bad(format!("{} strokes (at most {MAX_STROKES})", strokes.len()));
    }
    let mut points = 0;
    for s in strokes {
        if !size_ok(s.size) {
            return bad(format!(
                "size {} must be above 0 and at most {MAX_SIZE}",
                s.size
            ));
        }
        if !(s.strength.is_finite() && (0.0..=1.0).contains(&s.strength)) {
            return bad(format!("strength {} must be 0..1", s.strength));
        }
        if s.points.is_empty() || s.points.len() % 2 != 0 {
            return bad("points must be x, y pairs, at least one".into());
        }
        if !s.points.iter().all(|&v| pos(v)) {
            return bad("a point lies far outside the picture".into());
        }
        points += s.points.len() / 2;
    }
    if points > MAX_POINTS {
        return bad(format!("{points} points (at most {MAX_POINTS})"));
    }
    Ok(())
}

/// A pull step, ready to run.
#[derive(Debug, Clone)]
pub struct Pull {
    /// The strokes, then the pins as the strokes they are built as.
    strokes: Vec<Stroke>,
    /// How many of `strokes` were drawn (the rest are pins).
    drawn: usize,
    smooth: bool,
}

impl Pull {
    /// Build from the step's parameters, strokes and pins (already
    /// validated). Pins run after the strokes.
    pub fn new(params: &Params, strokes: &[Stroke], pins: &[Pin]) -> Pull {
        Pull {
            strokes: strokes
                .iter()
                .cloned()
                .chain(pins.iter().map(Pin::as_stroke))
                .collect(),
            drawn: strokes.len(),
            smooth: params.choice("sampling") == "smooth",
        }
    }

    /// The same pull on a crop's canvas.
    pub fn reframe(&mut self, f: &Frame) {
        self.strokes = self.strokes.iter().map(|s| s.reframed(f)).collect();
    }

    /// Bend raster bytes measured by `units`: build the map for the canvas
    /// at full size, then read every pixel through it.
    pub fn apply(
        &self,
        bytes: &mut [u8],
        units: &Units,
        cancel: &(dyn Fn() -> bool + Sync),
    ) -> Result<()> {
        if self.strokes.is_empty() || units.width == 0 || units.height == 0 {
            return Ok(());
        }
        let map = self.map(units.full_width, units.full_height, cancel)?;
        if map.is_still() {
            return Ok(());
        }
        let input = bytes.to_vec();
        map.render(
            &input,
            bytes,
            units.width,
            units.height,
            self.smooth,
            cancel,
        )
    }
}

impl Pull {
    /// The map for a canvas of `cw` x `ch` at full size, from the longest
    /// checkpoint these strokes begin with, leaving checkpoints behind.
    fn map(&self, cw: usize, ch: usize, cancel: &(dyn Fn() -> bool + Sync)) -> Result<Arc<Map>> {
        let all = &self.strokes;
        let n = all.len();
        let (map, from) = CHECKPOINTS
            .start(cw, ch, all)
            .unwrap_or_else(|| (Arc::new(Map::new(cw, ch)), 0));
        if from == n {
            return Ok(map); // the very map: nothing to add, nothing copied
        }
        let mut map = Arc::unwrap_or_clone(map);
        // Where this build first differs from one kept: in the middle of a
        // drag, where the moving stroke or pin is, so the next frame starts
        // there.
        let differs = CHECKPOINTS.shared(cw, ch, all);
        // Where a later build is likely to pick up (see the module notes).
        // A map already kept is not copied again, so the fixed points cost
        // a copy only now and then; a pin being moved (the last one
        // included) is covered by `differs`.
        let keep = |k: usize| {
            (k == differs && k > from)
                || (self.drawn > 0 && k == self.drawn - 1)
                || (n > self.drawn && k == self.drawn)
        };
        for (k, s) in all.iter().enumerate().skip(from) {
            if cancel() {
                return Err(Error::Cancelled);
            }
            if keep(k) {
                CHECKPOINTS.keep(cw, ch, &all[..k], &map);
            }
            map.stroke(s);
        }
        let map = Arc::new(map);
        CHECKPOINTS.keep_arc(cw, ch, all, map.clone());
        Ok(map)
    }
}

/// Maps kept at checkpoints, newest first (see the module notes). Each is
/// up to 1024 x 1024 points of two f64s (16 MB), so only a few are kept.
struct Checkpoints {
    items: Mutex<Vec<Checkpoint>>,
}

struct Checkpoint {
    /// The canvas at full size, which decides the map's size.
    canvas: (usize, usize),
    /// The strokes the map holds, in order.
    strokes: Vec<Stroke>,
    map: Arc<Map>,
}

const KEPT: usize = 6;

static CHECKPOINTS: Checkpoints = Checkpoints {
    items: Mutex::new(Vec::new()),
};

impl Checkpoints {
    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Checkpoint>> {
        self.items.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The longest kept map that `strokes` begin with, and how many strokes
    /// it holds. Every kept map they begin with moves to the front, the
    /// longest first: the shorter ones (the strokes before the pins, say)
    /// are where the next change may pick up, so the stream of whole maps
    /// a drag leaves must not push them out.
    fn start(&self, cw: usize, ch: usize, strokes: &[Stroke]) -> Option<(Arc<Map>, usize)> {
        let mut items = self.lock();
        let (mut ancestors, rest): (Vec<Checkpoint>, Vec<Checkpoint>) =
            items.drain(..).partition(|c| {
                c.canvas == (cw, ch)
                    && c.strokes.len() <= strokes.len()
                    && c.strokes[..] == strokes[..c.strokes.len()]
            });
        ancestors.sort_by_key(|c| std::cmp::Reverse(c.strokes.len()));
        let found = ancestors.first().map(|c| (c.map.clone(), c.strokes.len()));
        items.extend(ancestors);
        items.extend(rest);
        found
    }

    /// Keep a copy of `map`, holding `strokes`.
    fn keep(&self, cw: usize, ch: usize, strokes: &[Stroke], map: &Map) {
        if self.has(cw, ch, strokes) {
            return;
        }
        self.keep_arc(cw, ch, strokes, Arc::new(map.clone()));
    }

    /// The most strokes `strokes` has in common, from the start, with any
    /// kept map's strokes.
    fn shared(&self, cw: usize, ch: usize, strokes: &[Stroke]) -> usize {
        self.lock()
            .iter()
            .filter(|c| c.canvas == (cw, ch))
            .map(|c| {
                c.strokes
                    .iter()
                    .zip(strokes)
                    .take_while(|(a, b)| a == b)
                    .count()
            })
            .max()
            .unwrap_or(0)
    }

    fn has(&self, cw: usize, ch: usize, strokes: &[Stroke]) -> bool {
        self.lock()
            .iter()
            .any(|c| c.canvas == (cw, ch) && c.strokes[..] == strokes[..])
    }

    /// Drop every map (timings that must start cold).
    #[cfg(test)]
    fn forget(&self) {
        self.lock().clear();
    }

    fn keep_arc(&self, cw: usize, ch: usize, strokes: &[Stroke], map: Arc<Map>) {
        let mut items = self.lock();
        items.retain(|c| !(c.canvas == (cw, ch) && c.strokes[..] == strokes[..]));
        items.insert(
            0,
            Checkpoint {
                canvas: (cw, ch),
                strokes: strokes.to_vec(),
                map,
            },
        );
        items.truncate(KEPT);
    }
}

/// The displacement map: for each point, where to read, as an offset in
/// map cells. Point `(i, j)` sits at the centre of its cell, `(i + 0.5,
/// j + 0.5)` in map coordinates.
#[derive(Clone)]
struct Map {
    w: usize,
    h: usize,
    /// x and y offsets, interleaved, row by row.
    d: Vec<f64>,
}

/// Round half up to a whole number, with a floor of 1.
fn round_up_half(v: f64) -> usize {
    ((v + 0.5).floor() as usize).max(1)
}

impl Map {
    /// A still map for a canvas of `cw` x `ch` pixels at full size.
    fn new(cw: usize, ch: usize) -> Map {
        let long = cw.max(ch).max(1) as f64;
        let side = MAP_SIDE as f64;
        let w = round_up_half(side * cw as f64 / long);
        let h = round_up_half(side * ch as f64 / long);
        Map {
            w,
            h,
            d: vec![0.0; w * h * 2],
        }
    }

    fn is_still(&self) -> bool {
        self.d.iter().all(|&v| v == 0.0)
    }

    /// The offset at a map coordinate, interpolated between the four
    /// nearest points; past the edges, the edge's offset.
    fn at(&self, x: f64, y: f64) -> (f64, f64) {
        let (i0, i1, tx) = Self::span(x - 0.5, self.w);
        let (j0, j1, ty) = Self::span(y - 0.5, self.h);
        let get = |i: usize, j: usize| {
            let k = (j * self.w + i) * 2;
            (self.d[k], self.d[k + 1])
        };
        let (a, b, c, e) = (get(i0, j0), get(i1, j0), get(i0, j1), get(i1, j1));
        let top = (a.0 + (b.0 - a.0) * tx, a.1 + (b.1 - a.1) * tx);
        let bottom = (c.0 + (e.0 - c.0) * tx, c.1 + (e.1 - c.1) * tx);
        (
            top.0 + (bottom.0 - top.0) * ty,
            top.1 + (bottom.1 - top.1) * ty,
        )
    }

    /// The two grid indices around `v` (in point units) and the share of
    /// the way from the first to the second, clamped to `0..n`.
    fn span(v: f64, n: usize) -> (usize, usize, f64) {
        let last = (n - 1) as f64;
        let v = v.clamp(0.0, last);
        let i0 = v.floor();
        let i1 = (i0 + 1.0).min(last);
        (i0 as usize, i1 as usize, v - i0)
    }

    /// Apply one stroke: walk its path in steps of a quarter radius.
    fn stroke(&mut self, s: &Stroke) {
        let (w, h) = (self.w as f64, self.h as f64);
        let radius = s.size / 2.0 * w.max(h);
        if radius <= 0.0 || s.strength == 0.0 {
            return;
        }
        let spacing = (radius / 4.0).max(0.25);
        let pts: Vec<(f64, f64)> = s
            .points
            .chunks_exact(2)
            .map(|q| (q[0] * w, q[1] * h))
            .collect();
        for seg in pts.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len == 0.0 {
                continue;
            }
            let n = (len / spacing).ceil().max(1.0) as usize;
            let mut from = a;
            for k in 1..=n {
                let f = k as f64 / n as f64;
                let to = (a.0 + dx * f, a.1 + dy * f);
                self.step(from, to, radius, s.strength);
                from = to;
            }
        }
    }

    /// One small move of the brush from `a` to `b`. New offsets are worked
    /// out from the map as it stood, then written, so the step never reads
    /// its own output.
    fn step(&mut self, a: (f64, f64), b: (f64, f64), radius: f64, strength: f64) {
        let (mx, my) = (b.0 - a.0, b.1 - a.1);
        let r2 = radius * radius;
        // Points whose centres (i + 0.5) lie within the radius of `b`.
        let lo = |c: f64, n: usize| ((c - radius - 0.5).ceil().max(0.0) as usize).min(n);
        let hi =
            |c: f64, n: usize| ((c + radius - 0.5).floor() + 1.0).clamp(0.0, n as f64) as usize;
        let (i0, i1) = (lo(b.0, self.w), hi(b.0, self.w));
        let (j0, j1) = (lo(b.1, self.h), hi(b.1, self.h));
        let mut updates = Vec::new();
        for j in j0..j1 {
            let py = j as f64 + 0.5;
            let ey = py - b.1;
            for i in i0..i1 {
                let px = i as f64 + 0.5;
                let ex = px - b.0;
                let t2 = (ex * ex + ey * ey) / r2;
                if t2 >= 1.0 {
                    continue;
                }
                let fall = 1.0 - t2;
                let wgt = strength * fall * fall;
                let (sx, sy) = (wgt * mx, wgt * my);
                // What was at `p - w m` comes to `p`.
                let (ox, oy) = self.at(px - sx, py - sy);
                updates.push(((j * self.w + i) * 2, ox - sx, oy - sy));
            }
        }
        for (k, x, y) in updates {
            self.d[k] = x;
            self.d[k + 1] = y;
        }
    }

    /// Read every pixel of a `rw` x `rh` raster from `input` through the
    /// map into `out`.
    fn render(
        &self,
        input: &[u8],
        out: &mut [u8],
        rw: usize,
        rh: usize,
        smooth: bool,
        cancel: &(dyn Fn() -> bool + Sync),
    ) -> Result<()> {
        // Map cells per raster pixel, and back.
        let (kx, ky) = (self.w as f64 / rw as f64, self.h as f64 / rh as f64);
        let (lx, ly) = (rw as f64 / self.w as f64, rh as f64 / self.h as f64);
        let (maxx, maxy) = ((rw - 1) as f64, (rh - 1) as f64);
        let px = |x: usize, y: usize| (y * rw + x) * 3;
        for y in 0..rh {
            if cancel() {
                return Err(Error::Cancelled);
            }
            let my = (y as f64 + 0.5) * ky;
            for x in 0..rw {
                let mx = (x as f64 + 0.5) * kx;
                let (dx, dy) = self.at(mx, my);
                // The source, in raster pixels (centres at whole numbers).
                let sx = ((mx + dx) * lx - 0.5).clamp(0.0, maxx);
                let sy = ((my + dy) * ly - 0.5).clamp(0.0, maxy);
                let o = px(x, y);
                if smooth {
                    let (x0, y0) = (sx.floor(), sy.floor());
                    let (tx, ty) = (sx - x0, sy - y0);
                    let (x0, y0) = (x0 as usize, y0 as usize);
                    let (x1, y1) = ((x0 + 1).min(rw - 1), (y0 + 1).min(rh - 1));
                    let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
                    for ch in 0..3 {
                        let v = |k: usize| input[k + ch] as f64;
                        let top = v(a) + (v(b) - v(a)) * tx;
                        let bottom = v(c) + (v(d) - v(c)) * tx;
                        let m = top + (bottom - top) * ty;
                        out[o + ch] = (m + 0.5).floor().clamp(0.0, 255.0) as u8;
                    }
                } else {
                    let i = px((sx + 0.5).floor() as usize, (sy + 0.5).floor() as usize);
                    out[o..o + 3].copy_from_slice(&input[i..i + 3]);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipe::NEVER;

    fn pull(strokes: Vec<Stroke>, smooth: bool) -> Pull {
        Pull {
            drawn: strokes.len(),
            strokes,
            smooth,
        }
    }

    /// A picture whose every pixel says where it came from.
    fn coords(w: usize, h: usize) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| [(i % w) as u8, (i / w) as u8, 0])
            .collect()
    }

    fn drag(from: (f64, f64), to: (f64, f64), size: f64, strength: f64) -> Stroke {
        Stroke {
            size,
            strength,
            points: vec![from.0, from.1, to.0, to.1],
        }
    }

    #[test]
    fn no_strokes_or_no_movement_changes_nothing() {
        let base = coords(64, 48);
        let units = Units::full(64, 48);
        for strokes in [
            vec![],
            vec![Stroke {
                size: 0.2,
                strength: 1.0,
                points: vec![0.5, 0.5],
            }],
            vec![drag((0.5, 0.5), (0.7, 0.5), 0.2, 0.0)],
        ] {
            let mut b = base.clone();
            pull(strokes, false).apply(&mut b, &units, NEVER).unwrap();
            assert_eq!(b, base);
        }
    }

    #[test]
    fn full_strength_carries_the_centre_with_the_finger() {
        // A 256 x 128 picture: the map is 1024 x 512, four cells a pixel.
        let (w, h) = (256, 128);
        let base = coords(w, h);
        let units = Units::full(w, h);
        let mut b = base.clone();
        // Drag from x = 100 px to x = 140 px along the middle row.
        let s = drag((100.5 / 256.0, 0.5), (140.5 / 256.0, 0.5), 0.5, 1.0);
        pull(vec![s], false).apply(&mut b, &units, NEVER).unwrap();
        let at = |x: usize, y: usize| (b[(y * w + x) * 3], b[(y * w + x) * 3 + 1]);
        // Where the finger stopped shows what was under it at the start.
        let (sx, sy) = at(140, 64);
        assert!((sx as i32 - 100).abs() <= 1 && sy == 64, "{:?}", (sx, sy));
        // Far from the brush nothing moved.
        assert_eq!(at(5, 5), (5, 5));
        assert_eq!(at(250, 120), (250, 120));
        // Every pixel still comes from the picture (nothing invented).
        assert_eq!(b.len(), base.len());
    }

    #[test]
    fn pixels_sampling_only_copies_pixels_and_smooth_blends() {
        let (w, h) = (64, 64);
        // Noise, not a ramp: on a ramp a rounded blend is the nearest pixel.
        let base = crate::effects::testutil::noise(w * h * 3, 5);
        let units = Units::full(w, h);
        let s = drag((0.3, 0.5), (0.6, 0.55), 0.4, 0.7);
        let mut hard = base.clone();
        pull(vec![s.clone()], false)
            .apply(&mut hard, &units, NEVER)
            .unwrap();
        // Every output pixel exists in the input as it is.
        let pixels: std::collections::HashSet<_> = base.chunks(3).collect();
        assert!(hard.chunks(3).all(|p| pixels.contains(p)));
        assert_ne!(hard, base);
        let mut soft = base.clone();
        pull(vec![s], true).apply(&mut soft, &units, NEVER).unwrap();
        assert_ne!(soft, hard);
    }

    #[test]
    fn the_map_depends_on_the_full_size_not_the_raster() {
        let units_full = Units::full(400, 300);
        let preview = Units {
            full_width: 400,
            full_height: 300,
            width: 100,
            height: 75,
        };
        let a = Map::new(units_full.full_width, units_full.full_height);
        let b = Map::new(preview.full_width, preview.full_height);
        assert_eq!((a.w, a.h), (1024, 768));
        assert_eq!((b.w, b.h), (a.w, a.h));
        // A tall canvas.
        let m = Map::new(3, 4000);
        assert_eq!((m.w, m.h), (1, 1024));
    }

    #[test]
    fn cancellation_stops_a_pull() {
        let mut b = coords(32, 32);
        let r = pull(vec![drag((0.2, 0.2), (0.8, 0.8), 0.3, 1.0)], false).apply(
            &mut b,
            &Units::full(32, 32),
            &|| true,
        );
        assert!(matches!(r, Err(Error::Cancelled)));
    }

    /// A pin puts the point under it where it was dropped, and is the same
    /// as a straight drag at full strength.
    #[test]
    fn a_pin_lands_its_point_where_it_was_put() {
        let (w, h) = (256, 128);
        let base = coords(w, h);
        let units = Units::full(w, h);
        let pin = Pin {
            size: 0.3,
            from: [100.5 / 256.0, 0.5],
            to: [130.5 / 256.0, 0.5 + 10.0 / 128.0],
        };
        let mut b = base.clone();
        let params = Params::new("pull", INFO.params, &Default::default()).unwrap();
        Pull::new(&params, &[], std::slice::from_ref(&pin))
            .apply(&mut b, &units, NEVER)
            .unwrap();
        let at = |x: usize, y: usize| (b[(y * w + x) * 3], b[(y * w + x) * 3 + 1]);
        let (sx, sy) = at(130, 74);
        assert!(
            (sx as i32 - 100).abs() <= 1 && (sy as i32 - 64).abs() <= 1,
            "{:?}",
            (sx, sy)
        );
        let mut c = base.clone();
        pull(
            vec![Stroke {
                size: 0.3,
                strength: 1.0,
                points: vec![pin.from[0], pin.from[1], pin.to[0], pin.to[1]],
            }],
            false,
        )
        .apply(&mut c, &units, NEVER)
        .unwrap();
        assert_eq!(b, c);
        // A pin put back where it was changes nothing.
        let mut d = base.clone();
        let home = Pin {
            to: pin.from,
            ..pin.clone()
        };
        Pull::new(&params, &[], &[home])
            .apply(&mut d, &units, NEVER)
            .unwrap();
        assert_eq!(d, base);
    }

    #[test]
    fn bad_pins_are_refused() {
        let ok = Pin {
            size: 0.1,
            from: [0.2, 0.2],
            to: [0.3, 0.2],
        };
        assert!(validate_pins(std::slice::from_ref(&ok)).is_ok());
        for bad in [
            Pin {
                size: 0.0,
                ..ok.clone()
            },
            Pin {
                size: 2.0,
                ..ok.clone()
            },
            Pin {
                to: [f64::NAN, 0.2],
                ..ok.clone()
            },
            Pin {
                from: [0.2, -40.0],
                ..ok.clone()
            },
        ] {
            assert!(
                validate_pins(std::slice::from_ref(&bad)).is_err(),
                "{bad:?}"
            );
        }
    }

    /// A map built from a checkpoint is the map built from nothing: drawing
    /// stroke by stroke, then moving a pin, gives the same bytes as a pull
    /// that never saw the earlier ones. (Other tests share the checkpoints;
    /// they can only ever hold true maps, so that does not matter.)
    #[test]
    fn checkpoints_give_the_same_picture() {
        let (w, h) = (97, 61); // a canvas no other test uses
        let base = crate::effects::testutil::noise(w * h * 3, 9);
        let units = Units::full(w, h);
        let params = Params::new("pull", INFO.params, &Default::default()).unwrap();
        let strokes = vec![
            drag((0.2, 0.3), (0.5, 0.4), 0.3, 0.7),
            drag((0.6, 0.6), (0.4, 0.2), 0.2, 1.0),
            drag((0.1, 0.8), (0.9, 0.7), 0.4, 0.5),
        ];
        let pin = |x: f64| Pin {
            size: 0.25,
            from: [0.5, 0.5],
            to: [x, 0.45],
        };
        let run = |s: &[Stroke], p: &[Pin]| {
            let mut b = base.clone();
            Pull::new(&params, s, p)
                .apply(&mut b, &units, NEVER)
                .unwrap();
            b
        };
        // Build up as the GUI would, leaving checkpoints.
        for k in 1..=3 {
            run(&strokes[..k], &[]);
        }
        run(&strokes, &[pin(0.6)]);
        let cached = run(&strokes, &[pin(0.7)]);
        // The same, built from nothing: a map made by hand, step by step.
        let mut map = Map::new(w, h);
        for s in strokes.iter().chain([&pin(0.7).as_stroke()]) {
            map.stroke(s);
        }
        let mut fresh = base.clone();
        let input = fresh.clone();
        map.render(&input, &mut fresh, w, h, false, NEVER).unwrap();
        assert_eq!(cached, fresh);
    }

    /// How the pull's cost grows with strokes and pins (8c). Not part of
    /// the gate: `cargo test --release -p glitchrx-core speed -- --ignored
    /// --nocapture`. A 12 MP canvas; finger-like strokes (40 points across
    /// a third of the photo) and pins moved a tenth of it, all 12% wide.
    #[test]
    #[ignore]
    fn speed() {
        use std::time::Instant;
        // A small, fixed pseudo-random walk, so runs compare.
        let mut x = 7u64;
        let mut rnd = move || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % 10_000) as f64 / 10_000.0
        };
        let mut strokes = Vec::new();
        for _ in 0..200 {
            let (sx, sy) = (0.1 + 0.5 * rnd(), 0.1 + 0.5 * rnd());
            let (dx, dy) = (0.33 * (rnd() - 0.3), 0.33 * (rnd() - 0.3));
            let points = (0..40)
                .flat_map(|k| {
                    let f = k as f64 / 39.0;
                    [sx + dx * f, sy + dy * f]
                })
                .collect();
            strokes.push(Stroke {
                size: 0.12,
                strength: 0.6,
                points,
            });
        }
        let pins: Vec<Pin> = (0..100)
            .map(|_| {
                let (fx, fy) = (0.1 + 0.8 * rnd(), 0.1 + 0.8 * rnd());
                Pin {
                    size: 0.12,
                    from: [fx, fy],
                    to: [fx + 0.1 * (rnd() - 0.5), fy + 0.1 * (rnd() - 0.5)],
                }
            })
            .collect();
        let params = Params::new("pull", INFO.params, &Default::default()).unwrap();
        let (fw, fh) = (4000, 3000);
        let time = |s: &[Stroke], p: &[Pin], w: usize, h: usize| {
            let units = Units {
                full_width: fw,
                full_height: fh,
                width: w,
                height: h,
            };
            let mut bytes = coords(w, h);
            let pull = Pull::new(&params, s, p);
            CHECKPOINTS.forget();
            let t = Instant::now();
            pull.apply(&mut bytes, &units, NEVER).unwrap();
            t.elapsed().as_secs_f64() * 1000.0
        };
        println!("map only (8 x 6 raster), then 1024 x 768 preview, then 4000 x 3000 export, ms:");
        for n in [1, 10, 50, 100, 200] {
            println!(
                "{n:>4} strokes: {:>8.1} {:>8.1} {:>8.1}",
                time(&strokes[..n], &[], 8, 6),
                time(&strokes[..n], &[], 1024, 768),
                time(&strokes[..n], &[], fw, fh)
            );
        }
        for n in [1, 10, 50, 100] {
            println!(
                "{n:>4} pins:    {:>8.1} {:>8.1}",
                time(&[], &pins[..n], 8, 6),
                time(&[], &pins[..n], 1024, 768)
            );
        }

        // Live, as the GUI renders it: 50 strokes down, then one drawn a
        // few points a frame on the 1024 x 768 preview (checkpoints warm).
        let preview = Units {
            full_width: fw,
            full_height: fh,
            width: 1024,
            height: 768,
        };
        let frame = |s: &[Stroke], p: &[Pin]| {
            let mut bytes = coords(1024, 768);
            let t = Instant::now();
            Pull::new(&params, s, p)
                .apply(&mut bytes, &preview, NEVER)
                .unwrap();
            t.elapsed().as_secs_f64() * 1000.0
        };
        CHECKPOINTS.forget();
        let down = &strokes[..50];
        frame(down, &[]);
        let live = &strokes[50];
        let mut worst: f64 = 0.0;
        let mut total = 0.0;
        let frames = 20;
        for f in 1..=frames {
            let mut s = down.to_vec();
            s.push(Stroke {
                points: live.points[..(f * 2).max(4)].to_vec(),
                ..live.clone()
            });
            let ms = frame(&s, &[]);
            worst = worst.max(ms);
            total += ms;
        }
        println!(
            "drawing over 50 strokes: {:.1} ms a frame on average, {:.1} at worst",
            total / frames as f64,
            worst
        );
        // A pin moved over 50 strokes and 20 pins: the last, then the first.
        let some = &pins[..20];
        frame(down, some);
        let moved = |i: usize, dx: f64| {
            let mut p = some.to_vec();
            p[i].to[0] += dx;
            p
        };
        let last: f64 = (1..=10)
            .map(|k| frame(down, &moved(19, k as f64 * 0.002)))
            .sum::<f64>()
            / 10.0;
        let firsts: Vec<f64> = (1..=10)
            .map(|k| frame(down, &moved(0, k as f64 * 0.002)))
            .collect();
        println!("  first pin, frame by frame: {firsts:.0?}");
        let first = firsts.iter().sum::<f64>() / 10.0;
        println!(
            "moving a pin over 50 strokes and 20 pins: last {last:.1} ms, first {first:.1} ms a frame"
        );
        // The map alone for the first pin's moves (8 x 6 raster, no render).
        let tiny = Units {
            full_width: fw,
            full_height: fh,
            width: 8,
            height: 6,
        };
        let map_only: f64 = (11..=20)
            .map(|k| {
                let mut bytes = coords(8, 6);
                let p = moved(0, k as f64 * 0.002);
                let t = Instant::now();
                Pull::new(&params, down, &p)
                    .apply(&mut bytes, &tiny, NEVER)
                    .unwrap();
                t.elapsed().as_secs_f64() * 1000.0
            })
            .sum::<f64>()
            / 10.0;
        println!("  of which the map: {map_only:.1} ms");
    }

    #[test]
    fn bad_strokes_are_refused() {
        let ok = drag((0.1, 0.1), (0.2, 0.2), 0.1, 0.5);
        assert!(validate(std::slice::from_ref(&ok)).is_ok());
        for bad in [
            Stroke {
                size: 0.0,
                ..ok.clone()
            },
            Stroke {
                size: 1.5,
                ..ok.clone()
            },
            Stroke {
                strength: 1.2,
                ..ok.clone()
            },
            Stroke {
                strength: f64::NAN,
                ..ok.clone()
            },
            Stroke {
                points: vec![],
                ..ok.clone()
            },
            Stroke {
                points: vec![0.1, 0.2, 0.3],
                ..ok.clone()
            },
            Stroke {
                points: vec![0.1, 99.0],
                ..ok.clone()
            },
        ] {
            assert!(validate(std::slice::from_ref(&bad)).is_err(), "{bad:?}");
        }
    }
}
