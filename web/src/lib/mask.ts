// Mask editing helpers: pure functions the painter and the step card share.
//
// A mask is an ordered list of operations (strokes, gradients, brightness
// ranges), each added, subtracted or intersected with the mask so far. The
// order matters for intersections: a stroke painted *after* an intersect
// would escape it. So the editor keeps intersect operations at the end of
// the list, where they act as limits on everything painted. Recipes written
// by hand can still order operations any way they like.

import { SLIDER_STEPS, type SliderScale } from './slider';
import type { Mask, MaskMode, MaskOp } from './types';

/** Overlay colour for the mask view: Gruvbox red, like a rubylith. */
export const MASK_TINT: [number, number, number] = [204, 36, 29];

export type MaskTool = 'brush' | 'eraser' | 'linear' | 'radial';

/** Painting settings. Not part of the recipe; remembered per browser. */
export interface ToolState {
  tool: MaskTool;
  /** Brush diameter as a share of the picture's longer side. */
  size: number;
  /** 1 = crisp edge, 0 = soft all the way in. */
  hardness: number;
  /** Lay the mask over the preview while painting. */
  show: boolean;
}

export const DEFAULT_TOOL: ToolState = { tool: 'brush', size: 0.06, hardness: 0.5, show: true };

export const BRUSH_MIN = 0.005;
export const BRUSH_MAX = 0.3;

export const emptyMask = (): Mask => ({ invert: false, feather: 0, op: [] });

/** Add an operation, keeping intersect operations last. */
export function insertOp(ops: MaskOp[], op: MaskOp): MaskOp[] {
  if (op.mode === 'intersect') return [...ops, op];
  let i = ops.length;
  while (i > 0 && ops[i - 1].mode === 'intersect') i--;
  return [...ops.slice(0, i), op, ...ops.slice(i)];
}

/** Change an operation's mode; one that becomes an intersect moves last. */
export function withMode(ops: MaskOp[], index: number, mode: MaskMode): MaskOp[] {
  const op = { ...ops[index], mode } as MaskOp;
  const rest = ops.filter((_, i) => i !== index);
  if (mode === 'intersect') return [...rest, op];
  // Leaving the intersect group: put it just before the group.
  if (ops[index].mode === 'intersect') return insertOp(rest, op);
  return ops.map((o, i) => (i === index ? op : o));
}

/** A row of the operation list: strokes are grouped, others stand alone. */
export type OpRow =
  | { kind: 'strokes'; painted: number; erased: number; indices: number[] }
  | { kind: 'op'; index: number; op: MaskOp };

export function opRows(ops: MaskOp[]): OpRow[] {
  const rows: OpRow[] = [];
  let strokes: Extract<OpRow, { kind: 'strokes' }> | null = null;
  ops.forEach((op, index) => {
    if (op.shape === 'stroke') {
      if (!strokes) {
        strokes = { kind: 'strokes', painted: 0, erased: 0, indices: [] };
        rows.push(strokes);
      }
      strokes.indices.push(index);
      if (op.mode === 'subtract') strokes.erased++;
      else strokes.painted++;
    } else {
      rows.push({ kind: 'op', index, op });
    }
  });
  return rows;
}

/** Positions in a recipe: a ten-thousandth of the side (0.6 px at 6000 px). */
export const roundCoord = (v: number) => Math.round(v * 10000) / 10000;

/**
 * Add a point to a stroke (flat x, y list in picture fractions) if it is at
 * least `minPx` screen pixels from the last one, on a `w` x `h` display.
 * Keeps strokes short without visibly changing their path.
 */
export function addPoint(points: number[], x: number, y: number, minPx: number, w: number, h: number): boolean {
  const n = points.length;
  if (n >= 2) {
    const dx = (x - points[n - 2]) * w;
    const dy = (y - points[n - 1]) * h;
    if (dx * dx + dy * dy < minPx * minPx) return false;
  }
  points.push(roundCoord(x), roundCoord(y));
  return true;
}

/** Brush size slider: logarithmic, so small brushes get room. */
export const brushScale: SliderScale = {
  toSlider: (v) => (Math.log(clamp(v, BRUSH_MIN, BRUSH_MAX) / BRUSH_MIN) / Math.log(BRUSH_MAX / BRUSH_MIN)) * SLIDER_STEPS,
  fromSlider: (t) => roundTo(BRUSH_MIN * (BRUSH_MAX / BRUSH_MIN) ** (t / SLIDER_STEPS), 4),
};

/** Feather slider, 0 to 10% of the long side, finer near zero (quadratic). */
export const FEATHER_MAX = 0.1;
export const featherScale: SliderScale = {
  toSlider: (v) => Math.sqrt(clamp(v, 0, FEATHER_MAX) / FEATHER_MAX) * SLIDER_STEPS,
  fromSlider: (t) => roundTo(FEATHER_MAX * (t / SLIDER_STEPS) ** 2, 4),
};

/** A share as a percentage for display: 0.0125 -> "1.3%". */
export function percent(v: number): string {
  const p = v * 100;
  return `${p < 10 ? p.toFixed(1) : Math.round(p)}%`;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

function roundTo(v: number, digits: number): number {
  const f = 10 ** digits;
  return Math.round(v * f) / f;
}
