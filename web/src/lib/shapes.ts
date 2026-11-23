// Rectangle and pen path editing (SPEC 4b): what a press grabs, and what
// dragging it does. Pure geometry, kept apart from the painter so it can be
// tested. Positions here are display pixels from the picture's top-left
// corner (`w` x `h` is the picture's size on screen); the recipe stores
// fractions, as for every mask component.
//
// A rectangle has a centre, a size (shares of the longer side) and an angle
// (degrees, clockwise). Its corners resize it against the opposite corner,
// the handle above its top edge turns it, and dragging inside moves it.
//
// A pen path is a list of points, each a corner [x, y] or a smooth point
// [x, y, hx, hy] whose handle bends the side after it (and, mirrored, the
// side before). The outline always closes, last point to first.

import { roundCoord } from './mask';
import type { MaskOp } from './types';

export type Pt = [number, number];
export type RectOp = Extract<MaskOp, { shape: 'rect' }>;
/** [x, y] for a corner, [x, y, hx, hy] for a smooth point. */
export type PathPoint = number[];

const dist = (a: Pt, b: Pt) => Math.hypot(a[0] - b[0], a[1] - b[1]);
const frac = (q: Pt, w: number, h: number): Pt => [roundCoord(q[0] / w), roundCoord(q[1] / h)];

// ---------------------------------------------------------------------------
// Rectangles

/** How far the turn handle sits above the top edge, in CSS pixels. */
export const ROTATE_GAP = 30;

export interface RectGeom {
  centre: Pt;
  /** Half the width and height, in pixels. */
  hu: number;
  hv: number;
  /** Unit vectors along the width and the height, turned with the shape. */
  u: Pt;
  v: Pt;
  /** Top left, top right, bottom right, bottom left (before turning). */
  corners: Pt[];
  /** The middle of the top edge, and the turn handle above it. */
  top: Pt;
  rotate: Pt;
}

export function rectGeom(op: Pick<RectOp, 'center' | 'size' | 'angle'>, w: number, h: number): RectGeom {
  const long = Math.max(w, h);
  const centre: Pt = [op.center[0] * w, op.center[1] * h];
  const a = ((op.angle ?? 0) * Math.PI) / 180;
  // Clockwise on screen (y points down), as the server draws it.
  const u: Pt = [Math.cos(a), Math.sin(a)];
  const v: Pt = [-Math.sin(a), Math.cos(a)];
  const hu = (op.size[0] * long) / 2;
  const hv = (op.size[1] * long) / 2;
  const at = (su: number, sv: number): Pt => [
    centre[0] + su * hu * u[0] + sv * hv * v[0],
    centre[1] + su * hu * u[1] + sv * hv * v[1],
  ];
  const top = at(0, -1);
  return {
    centre,
    hu,
    hv,
    u,
    v,
    corners: [at(-1, -1), at(1, -1), at(1, 1), at(-1, 1)],
    top,
    rotate: [top[0] - v[0] * ROTATE_GAP, top[1] - v[1] * ROTATE_GAP],
  };
}

/** What a press on a rectangle grabs: a corner (0-3), the turn handle, its inside, or nothing (draw it again). */
export type RectGrab = { kind: 'corner'; k: number } | { kind: 'rotate' } | { kind: 'move' } | { kind: 'draw' };

/** Just the handle dots (corners and the turn handle), checked before other components' pins. */
export function grabRectHandle(op: RectOp, p: Pt, w: number, h: number, tol: number): RectGrab | null {
  const g = rectGeom(op, w, h);
  if (dist(p, g.rotate) < tol) return { kind: 'rotate' };
  let best: number | null = null;
  let bestD = tol;
  g.corners.forEach((c, k) => {
    const d = dist(p, c);
    if (d < bestD) [best, bestD] = [k, d];
  });
  return best === null ? null : { kind: 'corner', k: best };
}

export function grabRect(op: RectOp, p: Pt, w: number, h: number, tol: number): RectGrab {
  const handle = grabRectHandle(op, p, w, h, tol);
  if (handle) return handle;
  const g = rectGeom(op, w, h);
  const d: Pt = [p[0] - g.centre[0], p[1] - g.centre[1]];
  const du = Math.abs(d[0] * g.u[0] + d[1] * g.u[1]);
  const dv = Math.abs(d[0] * g.v[0] + d[1] * g.v[1]);
  return du <= g.hu && dv <= g.hv ? { kind: 'move' } : { kind: 'draw' };
}

/**
 * An angle in degrees, between -180 and 180, to a hundredth. With `snap`
 * (shift held) it goes in 15 degree steps; otherwise it settles on a
 * multiple of 45 within 2 degrees, so square is easy to find.
 */
export function niceAngle(deg: number, snap: boolean): number {
  let a = snap ? Math.round(deg / 15) * 15 : deg;
  const k = Math.round(a / 45) * 45;
  if (Math.abs(a - k) < 2) a = k;
  a = ((((a + 180) % 360) + 360) % 360) - 180;
  if (a === -180) a = 180;
  return Math.round(a * 100) / 100 || 0;
}

/** The rectangle after dragging what was grabbed from `start` to `p`. */
export function dragRect(
  g0: RectGrab,
  orig: RectOp,
  start: Pt,
  p: Pt,
  w: number,
  h: number,
  snap = false,
): Pick<RectOp, 'center' | 'size' | 'angle'> {
  const long = Math.max(w, h);
  const g = rectGeom(orig, w, h);
  const angle = orig.angle ?? 0;
  // A rectangle with one corner at `o` and the opposite one at `q`, keeping its angle.
  const span = (o: Pt, q: Pt) => {
    const d: Pt = [q[0] - o[0], q[1] - o[1]];
    const du = Math.abs(d[0] * g.u[0] + d[1] * g.u[1]);
    const dv = Math.abs(d[0] * g.v[0] + d[1] * g.v[1]);
    return {
      center: frac([(o[0] + q[0]) / 2, (o[1] + q[1]) / 2], w, h),
      size: [roundCoord(du / long), roundCoord(dv / long)] as [number, number],
      angle,
    };
  };
  switch (g0.kind) {
    case 'move':
      return { center: frac([g.centre[0] + p[0] - start[0], g.centre[1] + p[1] - start[1]], w, h), size: orig.size, angle };
    case 'rotate': {
      // The handle sits "up" from the centre: an angle of 0 points to -y.
      const deg = (Math.atan2(p[0] - g.centre[0], -(p[1] - g.centre[1])) * 180) / Math.PI;
      return { center: orig.center, size: orig.size, angle: niceAngle(deg, snap) };
    }
    case 'corner':
      return span(g.corners[(g0.k + 2) % 4], p);
    case 'draw':
      return span(start, p);
  }
}

// ---------------------------------------------------------------------------
// Pen paths

export const isSmooth = (q: PathPoint) => q.length === 4;
export const anchorPx = (q: PathPoint, w: number, h: number): Pt => [q[0] * w, q[1] * h];

/** A smooth point's handles: `out` (stored, bends the next side) and `in` (its mirror). */
export function handlesPx(q: PathPoint, w: number, h: number): { out: Pt; in: Pt } | null {
  if (!isSmooth(q)) return null;
  const a = anchorPx(q, w, h);
  const out: Pt = [q[2] * w, q[3] * h];
  return { out, in: [2 * a[0] - out[0], 2 * a[1] - out[1]] };
}

/** One side as a cubic: from `a` with controls `b`, `c` to `d`. Straight when both ends are corners. */
export interface Side {
  a: Pt;
  b: Pt;
  c: Pt;
  d: Pt;
  curved: boolean;
}

/** The sides from each point to the next; `closed` adds the one back to the start. */
export function sides(points: PathPoint[], w: number, h: number, closed = true): Side[] {
  const n = points.length;
  const out: Side[] = [];
  const last = closed ? n : n - 1;
  for (let i = 0; i < last && n > 1; i++) {
    const p = points[i];
    const q = points[(i + 1) % n];
    const a = anchorPx(p, w, h);
    const d = anchorPx(q, w, h);
    out.push({ a, b: handlesPx(p, w, h)?.out ?? a, c: handlesPx(q, w, h)?.in ?? d, d, curved: isSmooth(p) || isSmooth(q) });
  }
  return out;
}

/** A point on a side, `t` from 0 to 1. */
export function onSide(s: Side, t: number): Pt {
  const m = 1 - t;
  const [w0, w1, w2, w3] = [m * m * m, 3 * m * m * t, 3 * m * t * t, t * t * t];
  return [w0 * s.a[0] + w1 * s.b[0] + w2 * s.c[0] + w3 * s.d[0], w0 * s.a[1] + w1 * s.b[1] + w2 * s.c[1] + w3 * s.d[1]];
}

/** Steps a curved side is cut into for hit tests (the server flattens finer). */
const STEPS = 24;

/** The side's points from `a` to `d`, with the `t` of each. */
function samples(s: Side): { p: Pt; t: number }[] {
  const n = s.curved ? STEPS : 1;
  return Array.from({ length: n + 1 }, (_, k) => ({ p: onSide(s, k / n), t: k / n }));
}

/** Whether `p` is inside the closed outline (nonzero winding, as the server fills). */
export function insidePath(points: PathPoint[], p: Pt, w: number, h: number): boolean {
  if (points.length < 2) return false;
  let winding = 0;
  for (const s of sides(points, w, h)) {
    const pts = samples(s).map((x) => x.p);
    for (let k = 0; k + 1 < pts.length; k++) {
      const [a, b] = [pts[k], pts[k + 1]];
      const cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
      if (a[1] <= p[1] && b[1] > p[1] && cross > 0) winding++;
      else if (a[1] > p[1] && b[1] <= p[1] && cross < 0) winding--;
    }
  }
  return winding !== 0;
}

/** The closest place on the outline to `p`: which side, how far along, and how far away. */
function nearestOnOutline(points: PathPoint[], p: Pt, w: number, h: number): { i: number; t: number; d: number } | null {
  let best: { i: number; t: number; d: number } | null = null;
  sides(points, w, h).forEach((s, i) => {
    const pts = samples(s);
    for (let k = 0; k + 1 < pts.length; k++) {
      const [a, b] = [pts[k].p, pts[k + 1].p];
      const dx = b[0] - a[0];
      const dy = b[1] - a[1];
      const len2 = dx * dx + dy * dy;
      const f = len2 > 0 ? Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2)) : 0;
      const d = dist(p, [a[0] + f * dx, a[1] + f * dy]);
      if (!best || d < best.d) best = { i, t: pts[k].t + f * (pts[k + 1].t - pts[k].t), d };
    }
  });
  return best;
}

/**
 * What a press on a finished path grabs, in order: the picked point's
 * handles, any point, a side (a tap there adds a point), the inside (moves
 * the whole path), or nothing.
 */
export type PenGrab =
  | { kind: 'handle'; i: number; end: 'out' | 'in' }
  | { kind: 'point'; i: number }
  | { kind: 'side'; i: number; t: number }
  | { kind: 'move' }
  | { kind: 'none' };

export function grabPath(points: PathPoint[], p: Pt, w: number, h: number, tol: number, picked: number | null): PenGrab {
  const hs = picked !== null && points[picked] ? handlesPx(points[picked], w, h) : null;
  if (hs && picked !== null) {
    if (dist(p, hs.out) < tol) return { kind: 'handle', i: picked, end: 'out' };
    if (dist(p, hs.in) < tol) return { kind: 'handle', i: picked, end: 'in' };
  }
  const point = pointAt(points, p, w, h, tol);
  if (point !== null) return { kind: 'point', i: point };
  const near = nearestOnOutline(points, p, w, h);
  if (near && near.d < tol * 0.75 && points.length > 1) return { kind: 'side', i: near.i, t: near.t };
  return insidePath(points, p, w, h) ? { kind: 'move' } : { kind: 'none' };
}

/** The point nearest `p` within `tol`. */
export function pointAt(points: PathPoint[], p: Pt, w: number, h: number, tol: number): number | null {
  let best: number | null = null;
  let bestD = tol;
  points.forEach((q, i) => {
    const d = dist(p, anchorPx(q, w, h));
    if (d < bestD) [best, bestD] = [i, d];
  });
  return best;
}

const shift = (q: PathPoint, dx: number, dy: number): PathPoint =>
  q.map((v, k) => roundCoord(v + (k % 2 === 0 ? dx : dy)));

/** Move point `i` (and its handle) by a drag of `dx`, `dy` pixels. */
export function movePoint(points: PathPoint[], i: number, dx: number, dy: number, w: number, h: number): PathPoint[] {
  return points.map((q, k) => (k === i ? shift(q, dx / w, dy / h) : q));
}

/** Move the whole path. */
export function moveAll(points: PathPoint[], dx: number, dy: number, w: number, h: number): PathPoint[] {
  return points.map((q) => shift(q, dx / w, dy / h));
}

/** Put one of point `i`'s handles at `p`; the other mirrors it. */
export function moveHandle(points: PathPoint[], i: number, end: 'out' | 'in', p: Pt, w: number, h: number): PathPoint[] {
  const a = anchorPx(points[i], w, h);
  const out: Pt = end === 'out' ? p : [2 * a[0] - p[0], 2 * a[1] - p[1]];
  return points.map((q, k) => (k === i ? [q[0], q[1], ...frac(out, w, h)] : q));
}

/**
 * While placing: a drag from a new point pulls out its handle, bending the
 * side into it and the one after. A drag shorter than `min` pixels leaves
 * a corner.
 */
export function pullHandle(points: PathPoint[], i: number, p: Pt, w: number, h: number, min = 6): PathPoint[] {
  const q = points[i];
  if (dist(p, anchorPx(q, w, h)) < min) return points.map((x, k) => (k === i ? [x[0], x[1]] : x));
  return moveHandle(points, i, 'out', p, w, h);
}

/**
 * Add a point on side `i` (between point i and the next), `t` along it.
 * On a curve the new point is smooth, its handle along the curve there,
 * so the outline barely changes.
 */
export function insertPoint(points: PathPoint[], i: number, t: number, w: number, h: number): PathPoint[] {
  const s = sides(points, w, h)[i];
  if (!s) return points;
  const at = onSide(s, t);
  let q: PathPoint = frac(at, w, h);
  if (s.curved) {
    // de Casteljau: the split point's own control points give the tangent.
    const lerp = (a: Pt, b: Pt, f: number): Pt => [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
    const bc = lerp(s.b, s.c, t);
    const right = lerp(bc, lerp(s.c, s.d, t), t);
    const left = lerp(lerp(s.a, s.b, t), bc, t);
    // One handle length for both sides (they mirror): the average.
    const len = (dist(left, at) + dist(right, at)) / 2;
    const dir = dist(right, left) > 0 ? [(right[0] - left[0]) / dist(right, left), (right[1] - left[1]) / dist(right, left)] : [0, 0];
    if (len > 0.5) q = [...q, ...frac([at[0] + dir[0] * len, at[1] + dir[1] * len], w, h)];
  }
  return [...points.slice(0, i + 1), q, ...points.slice(i + 1)];
}

/**
 * Make point `i` smooth (handles along the line between its neighbours, a
 * quarter of the way to the nearer one) or a corner again.
 */
export function toggleSmooth(points: PathPoint[], i: number, w: number, h: number): PathPoint[] {
  const q = points[i];
  if (!q) return points;
  if (isSmooth(q)) return points.map((x, k) => (k === i ? [x[0], x[1]] : x));
  const n = points.length;
  const a = anchorPx(q, w, h);
  const prev = anchorPx(points[(i - 1 + n) % n], w, h);
  const next = anchorPx(points[(i + 1) % n], w, h);
  let dir: Pt = [next[0] - prev[0], next[1] - prev[1]];
  const dl = Math.hypot(dir[0], dir[1]);
  dir = dl > 0 ? [dir[0] / dl, dir[1] / dl] : [1, 0];
  const len = Math.max(8, Math.min(dist(a, prev), dist(a, next)) / 4);
  return points.map((x, k) => (k === i ? [x[0], x[1], ...frac([a[0] + dir[0] * len, a[1] + dir[1] * len], w, h)] : x));
}

/** Where an inactive path's pin sits: the average of its points. */
export function pathPin(points: PathPoint[], w: number, h: number): Pt | null {
  if (!points.length) return null;
  const s = points.reduce((acc, q) => [acc[0] + q[0], acc[1] + q[1]], [0, 0]);
  return [(s[0] / points.length) * w, (s[1] / points.length) * h];
}
