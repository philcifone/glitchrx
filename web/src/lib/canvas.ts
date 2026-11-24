// The canvas size, worked out as the server does (Crop::rect and
// fit_size in the core): integer arithmetic after one rounding, so the GUI
// and the server agree to the pixel on how large the canvas is.

import type { Crop } from './types';

/** Fit `w` x `h` inside `maxW` x `maxH`, keeping the aspect and never enlarging. */
export function fitSize(w: number, h: number, maxW: number, maxH: number): [number, number] {
  w = Math.max(1, w);
  h = Math.max(1, h);
  if (w <= maxW && h <= maxH) return [w, h];
  if (w * maxH >= h * maxW) return [Math.max(1, maxW), Math.max(1, Math.floor((h * maxW) / w))];
  return [Math.max(1, Math.floor((w * maxH) / h)), Math.max(1, maxH)];
}

/** The work sizes offered under the photo (the longest side; null is full size). */
export const WORK_SIZES: { size: number | null; label: string }[] = [
  { size: null, label: 'Full' },
  { size: 4096, label: '4K' },
  { size: 2048, label: '2K' },
  { size: 1024, label: '1K' },
];

/**
 * The crop in whole pixels of a `w` x `h` photo, as the core's
 * Crop::rect: each edge rounded to the nearest pixel, at least one pixel,
 * inside the photo.
 */
export function cropRect(c: Crop | undefined, w: number, h: number): { x: number; y: number; w: number; h: number } {
  if (!c) return { x: 0, y: 0, w, h };
  const edge = (share: number, n: number) => Math.min(n, Math.max(0, Math.floor(share * n + 0.5)));
  const span = (a: number, len: number, n: number): [number, number] => {
    const lo = Math.min(edge(a, n), n - 1);
    const hi = Math.min(n, Math.max(lo + 1, edge(a + len, n)));
    return [lo, hi - lo];
  };
  const [x, cw] = span(c.x, c.w, w);
  const [y, ch] = span(c.y, c.h, h);
  return { x, y, w: cw, h: ch };
}

/**
 * Crop ratios offered: the two sides as [long, short] (null is free, [0, 0]
 * the photo's own). Which way round they apply, and how they read, follows
 * the portrait / landscape switch, as in iOS Photos: 4:5 upright, 5:4 lying.
 */
export const CROP_RATIOS: { id: string; sides: [number, number] | null }[] = [
  { id: 'free', sides: null },
  { id: 'original', sides: [0, 0] },
  { id: '1:1', sides: [1, 1] },
  { id: '5:4', sides: [5, 4] },
  { id: '16:9', sides: [16, 9] },
];

/** The ratio's label, the way round it applies. */
export function ratioLabel(sides: [number, number] | null, portrait: boolean): string {
  if (!sides) return 'Free';
  if (sides[0] === 0) return 'Original';
  const [a, b] = sides;
  return portrait ? `${b}:${a}` : `${a}:${b}`;
}

/** Width / height for the ratio, the way round it applies (null when free). */
export function ratioValue(sides: [number, number] | null, portrait: boolean, photo: number): number | null {
  if (!sides) return null;
  if (sides[0] === 0) return photo;
  const r = sides[0] / sides[1];
  return portrait ? 1 / r : r;
}
