// Pull mode's brush settings and the small helpers around pull strokes
// (SPEC 8.2, D-64). The strokes themselves live in the step (`pull`); the
// brush is per device, like the mask brush, and not part of the recipe.

import { SLIDER_STEPS, type SliderScale } from './slider';
import type { PullStroke } from './types';

export interface PullTool {
  /** Brush diameter at fit, as a share of the photo's longer side. */
  size: number;
  /** 0 to 1: how far the pixels at the centre follow the finger. */
  strength: number;
}

/** The core's default strength (pull.rs), and a brush that reads at a glance. */
export const DEFAULT_PULL_TOOL: PullTool = { size: 0.12, strength: 0.6 };

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

export function strokeCount(n: number): string {
  return n === 0 ? 'No strokes yet' : n === 1 ? '1 stroke' : `${n} strokes`;
}
