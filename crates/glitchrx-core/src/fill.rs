//! Filling closed outlines with exact, anti-aliased coverage (the mask's
//! rectangles and pen paths, SPEC 4b).
//!
//! Each pixel gets the share of its area that lies inside the outline, so
//! edges are anti-aliased without sampling more than once per pixel. The
//! technique is the signed-area accumulation used by font rasterizers
//! (font-rs, stb_truetype, Skia's analytic AA):
//!
//! 1. Every edge adds, to a per-row buffer, how much area it encloses in
//!    each cell it crosses, signed by its direction (down is +1, up -1).
//! 2. A running sum along the row turns those amounts into coverage: a cell
//!    to the right of a downward edge, and left of the upward edge that
//!    closes it, sums to 1.
//!
//! The absolute value of the sum, capped at 1, is the nonzero winding rule,
//! so a path that crosses itself is still filled everywhere it loops.
//!
//! Curves are flattened into line segments first, finely enough that no
//! segment strays more than a tenth of a pixel from the curve. Everything
//! is basic f64 operations (plus `sqrt`, which IEEE rounds exactly), so a
//! fill is the same bytes on every machine (SPEC D-25).

/// A point in pixels.
pub type Pt = (f64, f64);

/// How far (pixels) a flattened curve may stray from the true one.
const TOLERANCE: f64 = 0.1;
/// The most segments one curved side becomes, whatever its size.
const MAX_PIECES: usize = 4096;

/// A cubic Bezier from `a` to `d`, with control points `b` and `c`, as
/// points along it (without `a`, which the previous side ends on).
///
/// The number of pieces comes from Wang's formula: the curve's largest
/// second difference bounds how far a chord can bow from it.
pub fn flatten_cubic(a: Pt, b: Pt, c: Pt, d: Pt, out: &mut Vec<Pt>) {
    let dd = |p: Pt, q: Pt, r: Pt| {
        let (x, y) = (p.0 - 2.0 * q.0 + r.0, p.1 - 2.0 * q.1 + r.1);
        (x * x + y * y).sqrt()
    };
    let l = dd(a, b, c).max(dd(b, c, d));
    let n = (0.75 * l / TOLERANCE).sqrt().ceil();
    let n = if n.is_finite() {
        (n as usize).clamp(1, MAX_PIECES)
    } else {
        1
    };
    for k in 1..=n {
        let t = k as f64 / n as f64;
        let m = 1.0 - t;
        let (w0, w1, w2, w3) = (m * m * m, 3.0 * m * m * t, 3.0 * m * t * t, t * t * t);
        out.push((
            w0 * a.0 + w1 * b.0 + w2 * c.0 + w3 * d.0,
            w0 * a.1 + w1 * b.1 + w2 * c.1 + w3 * d.1,
        ));
    }
}

/// One edge, top to bottom, with the direction it was drawn in.
#[derive(Clone, Copy)]
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    /// +1 if drawn downward, -1 if upward.
    dir: f64,
}

impl Edge {
    fn new(a: Pt, b: Pt) -> Option<Edge> {
        if a.1 == b.1 {
            // Horizontal: it encloses no area of its own.
            return None;
        }
        Some(if a.1 < b.1 {
            Edge {
                x0: a.0,
                y0: a.1,
                x1: b.0,
                y1: b.1,
                dir: 1.0,
            }
        } else {
            Edge {
                x0: b.0,
                y0: b.1,
                x1: a.0,
                y1: a.1,
                dir: -1.0,
            }
        })
    }

    /// Where the edge is at height `y` (inside its span).
    fn x_at(&self, y: f64) -> f64 {
        self.x0 + (y - self.y0) * ((self.x1 - self.x0) / (self.y1 - self.y0))
    }
}

/// Split a segment where it crosses the left and right sides of the
/// raster, and move the parts outside onto those sides. A part left of the
/// picture still covers every pixel to its right, and one right of it none,
/// so the coverage inside stays exact while every edge lands in the buffer.
fn clip_x(a: Pt, b: Pt, w: f64, out: &mut Vec<Edge>) {
    let mut cuts = [0.0, 1.0, 1.0, 1.0];
    let mut n = 1;
    for side in [0.0, w] {
        let (lo, hi) = (a.0.min(b.0), a.0.max(b.0));
        if lo < side && side < hi {
            cuts[n] = (side - a.0) / (b.0 - a.0);
            n += 1;
        }
    }
    cuts[1..n].sort_by(f64::total_cmp);
    cuts[n] = 1.0;
    let at = |t: f64| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    for i in 0..n {
        let (p, q) = (at(cuts[i]), at(cuts[i + 1]));
        let p = if i == 0 { a } else { p };
        let q = if i + 1 == n { b } else { q };
        let mid = (p.0 + q.0) * 0.5;
        let (p, q) = if mid <= 0.0 {
            ((0.0, p.1), (0.0, q.1))
        } else if mid >= w {
            ((w, p.1), (w, q.1))
        } else {
            (p, q)
        };
        if let Some(e) = Edge::new(p, q) {
            out.push(e);
        }
    }
}

/// Fill the closed outlines `paths` (each implicitly closed: its last point
/// joins its first) into `c`, a `w` x `h` buffer that starts at 0. Each
/// pixel gets its covered share of area, 0..1.
pub fn fill(c: &mut [f32], w: usize, h: usize, paths: &[Vec<Pt>]) {
    let wf = w as f64;
    let mut edges = Vec::new();
    for p in paths {
        if p.len() < 3 {
            continue;
        }
        for i in 0..p.len() {
            clip_x(p[i], p[(i + 1) % p.len()], wf, &mut edges);
        }
    }
    if edges.is_empty() {
        return;
    }
    // Sweep down the rows, keeping the edges that reach the current one.
    edges.sort_by(|a, b| a.y0.total_cmp(&b.y0));
    let first = edges[0].y0.floor().max(0.0);
    let last = edges
        .iter()
        .map(|e| e.y1)
        .fold(f64::MIN, f64::max)
        .ceil()
        .min(h as f64);
    if first >= last {
        return;
    }
    let mut acc = vec![0f64; w + 2];
    let mut active: Vec<usize> = Vec::new();
    let mut next = 0;
    for y in first as usize..last as usize {
        let (top, bottom) = (y as f64, y as f64 + 1.0);
        while next < edges.len() && edges[next].y0 < bottom {
            active.push(next);
            next += 1;
        }
        active.retain(|&i| edges[i].y1 > top);
        acc.fill(0.0);
        for &i in &active {
            accumulate(&mut acc, &edges[i], top, bottom, wf);
        }
        let mut sum = 0.0;
        for (x, v) in c[y * w..(y + 1) * w].iter_mut().enumerate() {
            sum += acc[x];
            let a = sum.abs();
            // Rounding leaves crumbs like 1e-16 outside and 0.9999999 inside.
            *v = if a < 1e-9 {
                0.0
            } else if a > 1.0 - 1e-9 {
                1.0
            } else {
                a as f32
            };
        }
    }
}

/// Add one edge's share of the row between `top` and `bottom` to `acc`.
///
/// Within the row the edge runs from `xa` to `xb` and covers a height `d`
/// (signed). Cells wholly to its right get all of `d` (through the running
/// sum); the cells it passes through get the part of their area right of
/// it. This is font-rs's `draw_line`, row by row.
fn accumulate(acc: &mut [f64], e: &Edge, top: f64, bottom: f64, w: f64) {
    let (ya, yb) = (e.y0.max(top), e.y1.min(bottom));
    if yb <= ya {
        return;
    }
    let d = e.dir * (yb - ya);
    // Clipping already put the edge inside 0..w; this only absorbs rounding.
    let xa = e.x_at(ya).clamp(0.0, w);
    let xb = e.x_at(yb).clamp(0.0, w);
    let (x0, x1) = if xa < xb { (xa, xb) } else { (xb, xa) };
    let x0f = x0.floor();
    let x1c = x1.ceil();
    let i0 = x0f as usize;
    let i1 = x1c as usize;
    if i1 <= i0 + 1 {
        // Inside one cell: split by where the edge crosses it, on average.
        let mid = 0.5 * (x0 + x1) - x0f;
        acc[i0] += d * (1.0 - mid);
        acc[i0 + 1] += d * mid;
        return;
    }
    // Across several cells: the covered area grows as a quadratic in the
    // first and last cell and linearly in between.
    let s = 1.0 / (x1 - x0);
    let f0 = x0 - x0f;
    let a0 = 0.5 * s * (1.0 - f0) * (1.0 - f0);
    let f1 = x1 - x1c + 1.0;
    let am = 0.5 * s * f1 * f1;
    acc[i0] += d * a0;
    if i1 == i0 + 2 {
        acc[i0 + 1] += d * (1.0 - a0 - am);
    } else {
        let a1 = s * (1.5 - f0);
        acc[i0 + 1] += d * (a1 - a0);
        for v in &mut acc[i0 + 2..i1 - 1] {
            *v += d * s;
        }
        let a2 = a1 + (i1 - i0 - 3) as f64 * s;
        acc[i1 - 1] += d * (1.0 - a2 - am);
    }
    acc[i1] += d * am;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(w: usize, h: usize, paths: &[Vec<Pt>]) -> Vec<f32> {
        let mut c = vec![0f32; w * h];
        fill(&mut c, w, h, paths);
        c
    }

    fn area(c: &[f32]) -> f64 {
        c.iter().map(|&v| v as f64).sum()
    }

    #[test]
    fn a_square_on_the_grid_is_crisp() {
        let c = filled(
            10,
            10,
            &[vec![(2.0, 3.0), (6.0, 3.0), (6.0, 8.0), (2.0, 8.0)]],
        );
        for y in 0..10 {
            for x in 0..10 {
                let inside = (2..6).contains(&x) && (3..8).contains(&y);
                assert_eq!(c[y * 10 + x], if inside { 1.0 } else { 0.0 }, "{x},{y}");
            }
        }
    }

    #[test]
    fn coverage_is_the_exact_area() {
        // Off-grid, a triangle, and both windings: the sum is the area.
        let quad = vec![(1.25, 2.5), (7.75, 2.5), (7.75, 6.1), (1.25, 6.1)];
        assert!((area(&filled(10, 10, std::slice::from_ref(&quad))) - 6.5 * 3.6).abs() < 1e-5);
        let mut back = quad;
        back.reverse();
        assert!((area(&filled(10, 10, &[back])) - 6.5 * 3.6).abs() < 1e-5);
        let tri = vec![(1.3, 1.1), (8.9, 4.2), (3.7, 9.6)];
        let want: f64 = 0.5 * ((8.9f64 - 1.3) * (9.6 - 1.1) - (3.7 - 1.3) * (4.2 - 1.1)).abs();
        assert!((area(&filled(10, 10, &[tri])) - want).abs() < 1e-5);
        // Half a pixel: a thin sliver gets half coverage.
        let c = filled(
            4,
            1,
            &[vec![(1.5, 0.0), (2.0, 0.0), (2.0, 1.0), (1.5, 1.0)]],
        );
        assert_eq!(c, vec![0.0, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn outlines_past_the_edges_are_clipped() {
        // A square much larger than the raster covers all of it.
        let big = vec![(-50.0, -40.0), (60.0, -40.0), (60.0, 70.0), (-50.0, 70.0)];
        assert!(filled(8, 6, &[big]).iter().all(|&v| v == 1.0));
        // A diagonal that runs off the left edge: the area inside is exact.
        let tri = vec![(-4.0, 0.0), (4.0, 0.0), (-4.0, 8.0)];
        // Inside 0..8 x 0..8 it is the triangle (0,0) (4,0) (0,4): area 8.
        assert!((area(&filled(8, 8, &[tri])) - 8.0).abs() < 1e-5);
        // Entirely outside: nothing.
        let away = vec![(20.0, 1.0), (30.0, 1.0), (30.0, 5.0)];
        assert!(filled(8, 8, &[away]).iter().all(|&v| v == 0.0));
    }

    #[test]
    fn a_path_that_crosses_itself_fills_both_loops() {
        // A bow tie: the two triangles, with the nonzero rule.
        let bow = vec![(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)];
        let c = filled(10, 10, &[bow]);
        assert_eq!(c[5 * 10 + 1], 1.0, "left loop");
        assert_eq!(c[5 * 10 + 8], 1.0, "right loop");
        assert_eq!(c[10 + 5], 0.0, "between the loops");
        assert!((area(&c) - 50.0).abs() < 1e-5);
    }

    #[test]
    fn curves_flatten_close_to_the_circle() {
        // Four cubic quarter arcs approximate a circle of radius 40.
        let (cx, cy, r) = (50.0, 50.0, 40.0);
        let k = 0.552_284_75 * r;
        let mut pts = vec![(cx + r, cy)];
        let arcs = [
            ((cx + r, cy + k), (cx + k, cy + r), (cx, cy + r)),
            ((cx - k, cy + r), (cx - r, cy + k), (cx - r, cy)),
            ((cx - r, cy - k), (cx - k, cy - r), (cx, cy - r)),
            ((cx + k, cy - r), (cx + r, cy - k), (cx + r, cy)),
        ];
        let mut from = pts[0];
        for (b, c, d) in arcs {
            flatten_cubic(from, b, c, d, &mut pts);
            from = d;
        }
        pts.pop(); // back at the start
        let got = area(&filled(100, 100, &[pts]));
        // Chords cut inside the curve, by at most the tolerance: the area
        // lost is under the perimeter times that.
        let want = std::f64::consts::PI * r * r;
        let lost = want - got;
        assert!(
            lost >= 0.0 && lost < 2.0 * std::f64::consts::PI * r * TOLERANCE,
            "{got} vs {want}"
        );
    }

    #[test]
    fn a_straight_cubic_is_one_piece() {
        let mut out = Vec::new();
        flatten_cubic((0.0, 0.0), (0.0, 0.0), (10.0, 0.0), (10.0, 0.0), &mut out);
        // Collinear controls still bow a little in speed, not in shape;
        // either way it ends on the last point.
        assert_eq!(*out.last().unwrap(), (10.0, 0.0));
        let mut out = Vec::new();
        flatten_cubic((0.0, 0.0), (1.0, 1.0), (2.0, 2.0), (3.0, 3.0), &mut out);
        assert_eq!(out, vec![(3.0, 3.0)]);
    }
}
