// Pull mode's brush settings and the small helpers around pull strokes
// and pins (SPEC 8.2, D-64, D-67). The strokes and pins live in the step
// (`pull`, `pin`); the tool is per device, like the mask brush, and not
// part of the recipe.
//
// Pins snap to a grid: `grid` cells across the photo's longer side, the
// same size both ways, centred so the margins match. The grid is only a
// way to pick points; the pins keep plain positions, so changing the grid
// later moves nothing.

import { SLIDER_STEPS, type SliderScale } from './slider';
import type { PullPin, PullStroke } from './types';

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
}

/** The core's default strength (pull.rs), and a brush that reads at a glance. */
export const DEFAULT_PULL_TOOL: PullTool = { size: 0.12, strength: 0.6, mode: 'draw', grid: 8, show: 'dots' };
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

/** What a pull step holds, in words: "2 strokes, 1 pin". */
export function strokeCount(strokes: number, pins = 0): string {
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

/**
 * The pin a finger at `p` picks up: the nearest handle (where a pin was
 * put) or home (where it came from) within `reach`, handles first, else
 * -1. `at` turns a share of the photo into the same units as `p`.
 */
export function pinAt(pins: PullPin[], p: { x: number; y: number }, at: (q: [number, number]) => { x: number; y: number }, reach: number): number {
  let best = -1;
  let bestD = reach;
  for (const end of ['to', 'from'] as const) {
    pins.forEach((pin, i) => {
      const q = at(pin[end]);
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
