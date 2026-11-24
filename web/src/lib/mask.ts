// Mask editing helpers: pure functions shared by the painter and the step
// card, kept here so they can be tested.
//
// A mask is a stack of components (brushes, linear gradients, radials,
// rectangles, pen paths, brightness ranges), each added, subtracted or intersected with the mask
// so far. Every component stays editable: a radial keeps its centre and
// edge, so it can be dragged and resized later; a brush keeps its strokes,
// and its eraser only takes back its own paint.
//
// Order matters for intersections: a component added *after* an intersect
// would escape it. So the editor keeps intersect components at the end of
// the stack, where they act as limits on everything else. Recipes written
// by hand can still order them any way they like.

import { SLIDER_STEPS, type SliderScale } from './slider';
import type { Mask, MaskMode, MaskOp, MaskShape, Stroke } from './types';

/** Overlay colour for the mask view: Gruvbox red, like a rubylith. */
export const MASK_TINT: [number, number, number] = [204, 36, 29];

/** What a brush component's strokes do. */
export type BrushTool = 'paint' | 'erase';

/** Painting settings. Not part of the recipe; remembered per browser. */
export interface ToolState {
  tool: BrushTool;
  /** Brush diameter as a share of the picture's longer side. */
  size: number;
  /** 1 = crisp edge, 0 = soft all the way in. */
  hardness: number;
  /** Lay the mask over the preview while editing. */
  show: boolean;
}

export const DEFAULT_TOOL: ToolState = { tool: 'paint', size: 0.06, hardness: 0.5, show: true };

/** Settings from storage, with anything stale or broken replaced by defaults. */
export function cleanTool(saved: unknown): ToolState {
  const t = { ...DEFAULT_TOOL, ...(saved && typeof saved === 'object' ? saved : {}) } as ToolState;
  if (t.tool !== 'paint' && t.tool !== 'erase') t.tool = 'paint';
  return t;
}

export const BRUSH_MIN = 0.005;
export const BRUSH_MAX = 0.3;

export const emptyMask = (): Mask => ({ invert: false, op: [] });

/** A component's opacity and feather, with their defaults filled in. */
export const opOpacity = (op: MaskOp) => op.opacity ?? 1;
export const opFeather = (op: MaskOp) => op.feather ?? 0;

export const SHAPE_LABEL: Record<MaskShape, string> = {
  brush: 'Brush',
  linear: 'Linear',
  radial: 'Radial',
  rect: 'Rectangle',
  path: 'Pen',
  brightness: 'Brightness',
};

/** A short description for the component list. */
export function opDetail(op: MaskOp): string {
  switch (op.shape) {
    case 'brush': {
      const erased = op.stroke.filter((s) => s.erase).length;
      const painted = op.stroke.length - erased;
      if (op.stroke.length === 0) return 'no strokes yet';
      return `${painted} ${painted === 1 ? 'stroke' : 'strokes'}${erased ? `, ${erased} erased` : ''}`;
    }
    case 'brightness':
      return `${Math.round(op.low * 100)}-${Math.round(op.high * 100)}`;
    case 'path': {
      const n = op.points.length;
      return n === 0 ? 'no points yet' : `${n} ${n === 1 ? 'point' : 'points'}`;
    }
    case 'rect':
      return op.angle ? `turned ${Math.round(op.angle)}\u00b0` : '';
    default:
      return '';
  }
}

/**
 * A new component with sensible starting geometry, for a picture of
 * `aspect` = width / height. A radial starts as a circle in the middle
 * (radius a fifth of the longer side), a linear fades from a
 * quarter of the way down to three quarters.
 */
export function newComponent(shape: MaskShape, aspect: number, others: number): MaskOp {
  switch (shape) {
    case 'brush':
      return { shape, mode: 'add', stroke: [] };
    case 'linear':
      return { shape, mode: 'add', from: [0.5, 0.25], to: [0.5, 0.75] };
    case 'radial': {
      // Radius 0.2 of the longer side, as a fraction of the width.
      const r = 0.2 * Math.max(1, 1 / aspect);
      return { shape, mode: 'add', from: [0.5, 0.5], to: [roundCoord(0.5 + r), 0.5], hardness: 1 };
    }
    case 'rect':
      // Half the picture each way, centred.
      return {
        shape,
        mode: 'add',
        center: [0.5, 0.5],
        size: [roundCoord(0.5 * Math.min(1, aspect)), roundCoord(0.5 * Math.min(1, 1 / aspect))],
      };
    case 'path':
      return { shape, mode: 'add', points: [] };
    case 'brightness':
      // With other components, a brightness range is most useful as a limit.
      return { shape, mode: others > 0 ? 'intersect' : 'add', low: 0.5, high: 1, soft: 0.1 };
  }
}

/** Add a component, keeping intersections last. Returns where it went. */
export function insertOp(ops: MaskOp[], op: MaskOp): { ops: MaskOp[]; index: number } {
  if (op.mode === 'intersect') return { ops: [...ops, op], index: ops.length };
  let i = ops.length;
  while (i > 0 && ops[i - 1].mode === 'intersect') i--;
  return { ops: [...ops.slice(0, i), op, ...ops.slice(i)], index: i };
}

/**
 * Change a component's mode; one that becomes an intersection moves last.
 * `order[newIndex]` is the old index, so a selection can follow its
 * component.
 */
export function withMode(ops: MaskOp[], index: number, mode: MaskMode): { ops: MaskOp[]; order: number[] } {
  const op = { ...ops[index], mode } as MaskOp;
  const idx = ops.map((_, i) => i);
  const restIdx = idx.filter((i) => i !== index);
  const rest = restIdx.map((i) => ops[i]);
  if (mode === 'intersect') return { ops: [...rest, op], order: [...restIdx, index] };
  if (ops[index].mode === 'intersect') {
    // Leaving the intersect group: put it just before the group.
    const { ops: out, index: at } = insertOp(rest, op);
    return { ops: out, order: [...restIdx.slice(0, at), index, ...restIdx.slice(at)] };
  }
  return { ops: ops.map((o, i) => (i === index ? op : o)), order: idx };
}

/** Where a selected component ends up after some are removed (null if it was). */
export function afterRemoval(active: number | null, removed: number[]): number | null {
  if (active === null || removed.includes(active)) return null;
  return active - removed.filter((i) => i < active).length;
}

/**
 * Add copied components to a mask (intersections kept last). Returns the
 * new list and where the pasted ones went, so they can be selected.
 */
export function pasteOps(ops: MaskOp[], pasted: MaskOp[]): { ops: MaskOp[]; indices: number[] } {
  let out = ops;
  const marked = pasted.map((op) => structuredClone(op));
  for (const op of marked) out = insertOp(out, op).ops;
  // Insertion moves earlier pastes along, so find them by identity at the end.
  const indices = marked.map((op) => out.indexOf(op)).sort((a, b) => a - b);
  return { ops: out, indices };
}

/**
 * Masks from earlier builds of this branch, in the current shape:
 * - one `stroke` operation per stroke: runs of them become one brush each
 *   (an eraser stroke becomes an erase stroke of that brush);
 * - a feather for the whole mask: it moves onto each component that has
 *   none of its own (close, though not identical: each edge is blurred
 *   rather than the finished mask).
 */
export function migrateMask(mask: Mask): Mask {
  const { feather, ...rest } = mask as Mask & { feather?: number };
  const out = migrateStrokes(rest);
  if (feather) out.op = out.op?.map((op) => (op.feather === undefined ? { ...op, feather } : op));
  return out;
}

function migrateStrokes(mask: Mask): Mask {
  const out: MaskOp[] = [];
  let brush: Extract<MaskOp, { shape: 'brush' }> | null = null;
  for (const raw of mask.op ?? []) {
    const op = raw as MaskOp | { shape: 'stroke'; mode: MaskMode; size: number; hardness?: number; points: number[] };
    if (op.shape === 'stroke') {
      if (!brush) {
        brush = { shape: 'brush', mode: 'add', stroke: [] };
        out.push(brush);
      }
      const s: Stroke = { size: op.size, hardness: op.hardness ?? 0.5, points: op.points };
      if (op.mode === 'subtract') s.erase = true;
      brush.stroke.push(s);
    } else {
      brush = null;
      out.push(op);
    }
  }
  return { ...mask, op: out };
}

/**
 * Positions in a recipe: a hundred-thousandth of the side (0.06 px at
 * 6000 px), fine enough for strokes painted at 800% zoom.
 */
export const roundCoord = (v: number) => Math.round(v * 100000) / 100000;

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

// ---------------------------------------------------------------------------
// Handles: what a press on the photo grabs, and what dragging it does.
// Geometry here is in display pixels; shapes are stored as fractions.

type Pt = [number, number];
type Shape = Extract<MaskOp, { shape: 'linear' | 'radial' }>;

/**
 * What a press grabs. Radial: `edge` (the edge handle or the ring) resizes,
 * `move` (inside) moves. Linear: `from` / `to` (an end handle or its line
 * across the picture) moves that end, `move` (near the axis between them)
 * moves both. Anything else is `draw`: drag out a new placement.
 */
export type Grab = 'edge' | 'from' | 'to' | 'move' | 'draw';

const px = (p: [number, number], w: number, h: number): Pt => [p[0] * w, p[1] * h];
const dist = (a: Pt, b: Pt) => Math.hypot(a[0] - b[0], a[1] - b[1]);

export function grab(op: Shape, p: Pt, w: number, h: number, tol: number): Grab {
  const handle = grabHandle(op, p, w, h, tol);
  if (handle) return handle;
  const a = px(op.from, w, h);
  const b = px(op.to, w, h);
  if (op.shape === 'radial') {
    const r = dist(a, b);
    const d = dist(p, a);
    if (Math.abs(d - r) < tol) return 'edge';
    return d < r ? 'move' : 'draw';
  }
  // Along the drag direction: how far p is from each end's line.
  const len = dist(a, b) || 1;
  const [ux, uy] = [(b[0] - a[0]) / len, (b[1] - a[1]) / len];
  const t = (p[0] - a[0]) * ux + (p[1] - a[1]) * uy;
  const off = Math.abs((p[0] - a[0]) * -uy + (p[1] - a[1]) * ux);
  if (Math.abs(t) < tol) return 'from';
  if (Math.abs(t - len) < tol) return 'to';
  if (t > 0 && t < len && off < tol * 1.5) return 'move';
  return 'draw';
}

/**
 * Just the handle dots: a radial's centre (move) and edge (resize), a
 * linear's two ends. The painter checks these before other components'
 * pins, and pins before the rest of `grab` (a radial's whole inside, a
 * linear's lines), so a big shape never hides the pins under it.
 */
export function grabHandle(op: Shape, p: Pt, w: number, h: number, tol: number): Grab | null {
  const a = px(op.from, w, h);
  const b = px(op.to, w, h);
  // The far handle wins a tie: on a tiny shape it is the one to pull out.
  if (dist(p, b) < tol) return op.shape === 'radial' ? 'edge' : 'to';
  if (dist(p, a) < tol) return op.shape === 'radial' ? 'move' : 'from';
  return null;
}

/**
 * The shape's ends after dragging from `start` to `p` (display pixels),
 * given what was grabbed and the ends when the drag began.
 */
export function dragEnds(
  grabbed: Grab,
  from: [number, number],
  to: [number, number],
  start: Pt,
  p: Pt,
  w: number,
  h: number,
): { from: [number, number]; to: [number, number] } {
  const f = (q: Pt): [number, number] => [roundCoord(q[0] / w), roundCoord(q[1] / h)];
  const [dx, dy] = [(p[0] - start[0]) / w, (p[1] - start[1]) / h];
  const shift = (q: [number, number]): [number, number] => [roundCoord(q[0] + dx), roundCoord(q[1] + dy)];
  switch (grabbed) {
    case 'move':
      return { from: shift(from), to: shift(to) };
    case 'edge':
    case 'to':
      return { from, to: f(p) };
    case 'from':
      return { from: f(p), to };
    case 'draw':
      return { from: f(start), to: f(p) };
  }
}

/** Where a shape's pin sits (display pixels): a radial's centre, a linear's midpoint. */
export function pinOf(op: Shape, w: number, h: number): Pt {
  const a = px(op.from, w, h);
  if (op.shape === 'radial') return a;
  const b = px(op.to, w, h);
  return [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
}

// ---------------------------------------------------------------------------
// Slider scales

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

// ---------------------------------------------------------------------------
// Masks and the crop (D-52)
//
// Masks are kept as shares of the whole, uncropped photo, so they stay on
// the same part of it whatever the crop. The server moves them onto the
// cropped canvas to render (the core's mask::Frame); the editor instead
// draws on the rectangle where the whole photo would sit on screen, which
// reaches past the visible crop, so every share it measures is already a
// share of the photo.

/** The crop as the core's mask::Frame sees it. */
export interface PhotoFrame {
  /** The crop's corner, as shares of the photo. */
  x: number;
  y: number;
  /** Photo width over crop width, and the same for heights. */
  sx: number;
  sy: number;
  /** The photo's longer side over the crop's. */
  sl: number;
}

/** The frame for a crop of whole pixels out of a `pw` x `ph` photo. */
export function photoFrame(crop: { x: number; y: number; w: number; h: number }, pw: number, ph: number): PhotoFrame {
  return {
    x: crop.x / pw,
    y: crop.y / ph,
    sx: pw / crop.w,
    sy: ph / crop.h,
    sl: Math.max(pw, ph) / Math.max(crop.w, crop.h),
  };
}

/** Where the whole photo sits on screen, given where the cropped canvas sits. */
export function photoRect(
  canvas: { x: number; y: number; w: number; h: number },
  f: PhotoFrame,
): { x: number; y: number; w: number; h: number } {
  return {
    x: canvas.x - f.x * f.sx * canvas.w,
    y: canvas.y - f.y * f.sy * canvas.h,
    w: canvas.w * f.sx,
    h: canvas.h * f.sy,
  };
}

/** A component laid out on the canvas (a new one), moved to shares of the photo. */
export function toPhoto(op: MaskOp, f: PhotoFrame): MaskOp {
  const pt = (p: [number, number]): [number, number] => [roundCoord(p[0] / f.sx + f.x), roundCoord(p[1] / f.sy + f.y)];
  switch (op.shape) {
    case 'linear':
      return { ...op, from: pt(op.from), to: pt(op.to) };
    case 'radial':
      return { ...op, from: pt(op.from), to: pt(op.to) };
    case 'rect':
      return { ...op, center: pt(op.center), size: [roundCoord(op.size[0] / f.sl), roundCoord(op.size[1] / f.sl)] };
    default:
      // Brushes and paths start empty; brightness has no place.
      return op;
  }
}
