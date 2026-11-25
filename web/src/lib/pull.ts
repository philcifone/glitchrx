// Pull mode's brush settings and the small helpers around pull strokes
// and pins (SPEC 8.2, D-64, D-67, D-68). Strokes and pins ("marks") live
// in the step's `pull` list, in the order made, so each goes over what
// came before. The tool is per device, like the mask brush, and not part
// of the recipe.
//
// Strokes belong to brushes: the strokes drawn while a brush is selected
// join it, and share its size and strength. A stroke from before brushes
// has no `brush`; it counts as a brush of its own, keyed by its place in
// the list as a negative number, until a stroke joins it and it gets a
// real one.
//
// Pins snap to a grid: `grid` cells across the photo's longer side, the
// same size both ways, centred so the margins match. The grid is only a
// way to pick points; the pins keep plain positions, so changing the grid
// later moves nothing.

import { SLIDER_STEPS, type SliderScale } from './slider';
import type { PullMark, PullPin, PullStroke } from './types';

/** Draw strokes freehand, or pick up grid points as pins. */
export type PullMode = 'draw' | 'pins';
/** How the pin grid shows on the photo (never in the picture itself). */
export type GridShow = 'dots' | 'lines' | 'off';

export interface PullTool {
  /** Brush diameter at fit, as a share of the photo's longer side. */
  size: number;
  /** 0 to 1: how far the pixels at the centre follow the finger. */
  strength: number;
  mode: PullMode;
  /** Pin grid: cells across the photo's longer side. */
  grid: number;
  show: GridShow;
  /** Show a marker where each stroke ends (D-68); off, the photo is clear. */
  markers: boolean;
}

/** The core's default strength (pull.rs), and a brush that reads at a glance. */
export const DEFAULT_PULL_TOOL: PullTool = { size: 0.12, strength: 0.6, mode: 'draw', grid: 8, show: 'dots', markers: true };
export const GRID_MIN = 2;
export const GRID_MAX = 32;

export const PULL_MIN = 0.01;
/** Larger brushes than a mask's: a pull often moves a whole region. */
export const PULL_MAX = 0.6;
export const STRENGTH_MIN = 0.05;

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const roundTo = (v: number, places: number) => Math.round(v * 10 ** places) / 10 ** places;

/** Settings from storage, with anything stale or broken replaced by defaults. */
export function cleanPullTool(saved: unknown): PullTool {
  const s = (saved && typeof saved === 'object' ? saved : {}) as Partial<PullTool>;
  const num = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? v : d);
  return {
    size: clamp(num(s.size, DEFAULT_PULL_TOOL.size), PULL_MIN, PULL_MAX),
    strength: clamp(num(s.strength, DEFAULT_PULL_TOOL.strength), STRENGTH_MIN, 1),
    mode: s.mode === 'pins' ? 'pins' : 'draw',
    grid: Math.round(clamp(num(s.grid, DEFAULT_PULL_TOOL.grid), GRID_MIN, GRID_MAX)),
    show: s.show === 'lines' || s.show === 'off' ? s.show : 'dots',
    markers: s.markers !== false,
  };
}

/** Size slider: logarithmic, so small brushes get as much travel as large ones. */
export const pullSizeScale: SliderScale = {
  toSlider: (v) => (Math.log(clamp(v, PULL_MIN, PULL_MAX) / PULL_MIN) / Math.log(PULL_MAX / PULL_MIN)) * SLIDER_STEPS,
  fromSlider: (t) => roundTo(PULL_MIN * (PULL_MAX / PULL_MIN) ** (t / SLIDER_STEPS), 4),
};

/** `[` and `]`: one notch smaller or larger. */
export function stepSize(size: number, by: -1 | 1): number {
  return clamp(roundTo(size * (by < 0 ? 0.85 : 1.18), 4), PULL_MIN, PULL_MAX);
}

/** A stroke worth keeping: it has to move, or it pulls nothing. */
export function moves(s: PullStroke): boolean {
  return s.points.length >= 4;
}

/** A pin, not a stroke. */
export const isPin = (m: PullMark): m is PullPin => 'from' in m;

/**
 * What the card's sliders change (D-68): a brush (all its strokes, by its
 * key) or a pin (by its place in the list). None: the next stroke or pin.
 */
export type PullSel = { brush: number } | { pin: number };

/** The brush a mark belongs to (see the notes above), or null for a pin. */
export function brushKey(marks: PullMark[], i: number): number | null {
  const m = marks[i];
  if (!m || isPin(m)) return null;
  return m.brush ?? -(i + 1);
}

/** Where a brush's strokes are in the list. */
export function brushStrokes(marks: PullMark[], key: number): number[] {
  return marks.flatMap((_, i) => (brushKey(marks, i) === key ? [i] : []));
}

/** A number no brush in the list has yet. */
export function nextBrush(marks: PullMark[]): number {
  return marks.reduce((n, m) => (!isPin(m) && m.brush !== undefined ? Math.max(n, m.brush + 1) : n), 1);
}

/** What a pull step holds, in words: "2 strokes, 1 pin". */
export function strokeCount(marks: PullMark[]): string {
  const pins = marks.filter(isPin).length;
  const strokes = marks.length - pins;
  if (!strokes && !pins) return 'Nothing pulled yet';
  const say = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'}`;
  return [strokes && say(strokes, 'stroke'), pins && say(pins, 'pin')].filter(Boolean).join(', ');
}

/**
 * The grid's lines as shares of the photo, across and down, for a photo
 * of `w` x `h` (any unit) with `cells` cells across its longer side.
 */
export function gridAxes(w: number, h: number, cells: number): { xs: number[]; ys: number[] } {
  const spacing = Math.max(w, h) / cells;
  const axis = (len: number) => {
    const n = Math.floor(len / spacing + 1e-9);
    const margin = (len - n * spacing) / 2;
    return Array.from({ length: n + 1 }, (_, k) => (margin + k * spacing) / len);
  };
  return { xs: axis(w), ys: axis(h) };
}

/** The grid point nearest to `x`, `y` (shares of the photo). */
export function snap(x: number, y: number, axes: { xs: number[]; ys: number[] }): [number, number] {
  const near = (v: number, list: number[]) => list.reduce((a, b) => (Math.abs(b - v) < Math.abs(a - v) ? b : a));
  return [roundTo(near(x, axes.xs), 6), roundTo(near(y, axes.ys), 6)];
}

type At = (q: [number, number]) => { x: number; y: number };

/**
 * The pin a finger at `p` picks up, by its place in `marks`: the nearest
 * handle (where a pin was put) or home (where it came from) within
 * `reach`, handles first, else -1. `at` turns a share of the photo into
 * the same units as `p`.
 */
export function pinAt(marks: PullMark[], p: { x: number; y: number }, at: At, reach: number): number {
  let best = -1;
  let bestD = reach;
  for (const end of ['to', 'from'] as const) {
    marks.forEach((m, i) => {
      if (!isPin(m)) return;
      const q = at(m[end]);
      const d = Math.hypot(q.x - p.x, q.y - p.y);
      if (d <= bestD) {
        best = i;
        bestD = d;
      }
    });
    if (best >= 0) return best;
  }
  return best;
}

/** Where a stroke's marker sits: where the finger lifted. */
export function strokeEnd(s: PullStroke): [number, number] {
  const n = s.points.length;
  return [s.points[n - 2], s.points[n - 1]];
}

/** The stroke whose marker is nearest `p` within `reach`, by its place in `marks`, else -1. */
export function strokeAt(marks: PullMark[], p: { x: number; y: number }, at: At, reach: number): number {
  let best = -1;
  let bestD = reach;
  marks.forEach((m, i) => {
    if (isPin(m)) return;
    const q = at(strokeEnd(m));
    const d = Math.hypot(q.x - p.x, q.y - p.y);
    if (d <= bestD) {
      best = i;
      bestD = d;
    }
  });
  return best;
}
