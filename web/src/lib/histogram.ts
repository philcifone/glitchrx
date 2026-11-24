// Histogram drawing shared by the curves and levels editors (6b): the
// picture reaching a step, behind the controls, as in Photoshop.

import type { Histogram } from './api';

/** The counts to show for an adjustment's `channel` (luma for all three). */
export function countsFor(hist: Histogram, channel: string): number[] {
  if (channel === 'red') return hist.red;
  if (channel === 'green') return hist.green;
  if (channel === 'blue') return hist.blue;
  return hist.luma;
}

/**
 * The counts as a filled SVG path over a box 255 wide and `height` tall,
 * level 0 at the left and the floor at the bottom; the tallest bar reaches
 * `reach` of the height.
 *
 * Scaled to the tallest level away from the ends: clipped blacks and
 * whites pile up at 0 and 255 and would flatten everything else.
 */
export function histogramPath(counts: number[], height: number, reach = 0.9): string {
  const top = Math.max(1, ...counts.slice(1, 255));
  let d = `M0,${height}`;
  counts.forEach((n, i) => {
    d += `L${i},${(height - Math.min(1, n / top) * height * reach).toFixed(1)}`;
  });
  return `${d}L255,${height}Z`;
}

/**
 * The levels midtone handle: where along the black-to-white stretch (0..1)
 * an input comes out half way, for a `gamma`. Levels maps t to
 * t^(1/gamma), so that is 0.5^gamma: gamma 1 sits in the middle, a larger
 * gamma (lighter) sits to the left, as in Photoshop.
 */
export function gammaToShare(gamma: number): number {
  return Math.pow(0.5, gamma);
}

/** The gamma for a midtone handle at `share` (0..1) of the stretch. */
export function shareToGamma(share: number, min = 0.1, max = 10): number {
  const s = Math.min(Math.max(share, Math.pow(0.5, max)), Math.pow(0.5, min));
  const g = Math.log(s) / Math.log(0.5);
  return Math.round(Math.min(max, Math.max(min, g)) * 100) / 100;
}
