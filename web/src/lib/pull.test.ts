import { describe, expect, it } from 'vitest';
import {
  DEFAULT_PULL_TOOL,
  GRID_MAX,
  PULL_MAX,
  PULL_MIN,
  brushKey,
  brushStrokes,
  cleanPullTool,
  gridAxes,
  moves,
  nextBrush,
  pinAt,
  pullSizeScale,
  snap,
  stepSize,
  strokeAt,
  strokeCount,
} from './pull';
import type { PullMark, PullPin, PullStroke } from './types';
import { SLIDER_STEPS } from './slider';

describe('pull tool', () => {
  it('falls back to defaults for missing or broken settings', () => {
    expect(cleanPullTool(null)).toEqual(DEFAULT_PULL_TOOL);
    expect(cleanPullTool({ size: 'big', strength: Number.NaN })).toEqual(DEFAULT_PULL_TOOL);
    expect(cleanPullTool({ size: 5, strength: -1, mode: 'x', grid: 99, show: 'stars' })).toEqual({
      ...DEFAULT_PULL_TOOL,
      size: PULL_MAX,
      strength: 0.05,
      grid: GRID_MAX,
    });
    expect(cleanPullTool({ mode: 'pins', show: 'lines', grid: 5.6 })).toMatchObject({ mode: 'pins', show: 'lines', grid: 6 });
  });

  it('maps the size slider end to end, and back', () => {
    expect(pullSizeScale.fromSlider(0)).toBeCloseTo(PULL_MIN);
    expect(pullSizeScale.fromSlider(SLIDER_STEPS)).toBeCloseTo(PULL_MAX);
    const t = pullSizeScale.toSlider(0.12);
    expect(pullSizeScale.fromSlider(Math.round(t))).toBeCloseTo(0.12, 3);
  });

  it('steps the size within its range', () => {
    expect(stepSize(0.1, 1)).toBeGreaterThan(0.1);
    expect(stepSize(0.1, -1)).toBeLessThan(0.1);
    expect(stepSize(PULL_MAX, 1)).toBe(PULL_MAX);
    expect(stepSize(PULL_MIN, -1)).toBe(PULL_MIN);
  });

  it('keeps only strokes that move', () => {
    expect(moves({ size: 0.1, strength: 0.6, points: [0.5, 0.5] })).toBe(false);
    expect(moves({ size: 0.1, strength: 0.6, points: [0.5, 0.5, 0.6, 0.5] })).toBe(true);
  });

  const stroke = (brush?: number, end: [number, number] = [0.5, 0.5]): PullStroke => ({
    size: 0.1,
    strength: 0.6,
    points: [0.1, 0.1, ...end],
    ...(brush === undefined ? {} : { brush }),
  });
  const pin = (to: [number, number] = [0.6, 0.5], from: [number, number] = [0.5, 0.5]): PullPin => ({ size: 0.1, from, to });

  it('counts strokes and pins in words', () => {
    const n = (s: number, p: number): PullMark[] => [...Array.from({ length: s }, () => stroke()), ...Array.from({ length: p }, () => pin())];
    expect(strokeCount(n(0, 0))).toBe('Nothing pulled yet');
    expect(strokeCount(n(1, 0))).toBe('1 stroke');
    expect(strokeCount(n(3, 0))).toBe('3 strokes');
    expect(strokeCount(n(0, 1))).toBe('1 pin');
    expect(strokeCount(n(2, 3))).toBe('2 strokes, 3 pins');
  });

  it('groups strokes into brushes; one from before brushes is a brush of its own', () => {
    const marks: PullMark[] = [stroke(2), pin(), stroke(), stroke(2), stroke(5), stroke()];
    expect(brushKey(marks, 0)).toBe(2);
    expect(brushKey(marks, 1)).toBeNull();
    expect(brushKey(marks, 2)).toBe(-3);
    expect(brushStrokes(marks, 2)).toEqual([0, 3]);
    expect(brushStrokes(marks, -3)).toEqual([2]);
    expect(brushStrokes(marks, -6)).toEqual([5]);
    expect(nextBrush(marks)).toBe(6);
    expect(nextBrush([pin(), stroke()])).toBe(1);
  });

  it('finds a stroke by its marker, where the finger lifted', () => {
    const at = (q: [number, number]) => ({ x: q[0] * 100, y: q[1] * 100 });
    const marks: PullMark[] = [stroke(1, [0.5, 0.5]), pin([0.52, 0.5]), stroke(1, [0.8, 0.2])];
    expect(strokeAt(marks, { x: 51, y: 50 }, at, 5)).toBe(0); // the pin is not a stroke
    expect(strokeAt(marks, { x: 80, y: 23 }, at, 5)).toBe(2);
    expect(strokeAt(marks, { x: 10, y: 10 }, at, 5)).toBe(-1); // a stroke's start is no marker
  });

  it('lays a square grid over the photo, centred', () => {
    // 400 x 300 with 4 cells across: 100 apart, 3 cells down, no margin.
    const a = gridAxes(400, 300, 4);
    expect(a.xs).toEqual([0, 0.25, 0.5, 0.75, 1]);
    expect(a.ys.map((v) => v * 300)).toEqual([0, 100, 200, 300]);
    // 400 x 250: 2 cells down and 25 px margins top and bottom.
    const b = gridAxes(400, 250, 4);
    expect(b.ys.map((v) => Math.round(v * 250))).toEqual([25, 125, 225]);
  });

  it('snaps to the nearest grid point', () => {
    const a = gridAxes(400, 300, 4);
    expect(snap(0.3, 0.6, a)).toEqual([0.25, 2 / 3].map((v) => Math.round(v * 1e6) / 1e6));
    expect(snap(-1, 2, a)).toEqual([0, 1]);
  });

  it('picks up the nearest pin, by its handle before its home', () => {
    const at = (q: [number, number]) => ({ x: q[0] * 100, y: q[1] * 100 });
    // Places in the list of marks: a stroke ending on a handle is passed over.
    const marks: PullMark[] = [pin([0.6, 0.5], [0.5, 0.5]), stroke(1, [0.51, 0.5]), pin([0.52, 0.5], [0.2, 0.2])];
    expect(pinAt(marks, { x: 51, y: 50 }, at, 5)).toBe(2); // a handle beats a nearer home
    expect(pinAt(marks, { x: 60, y: 51 }, at, 5)).toBe(0);
    expect(pinAt(marks, { x: 21, y: 20 }, at, 5)).toBe(2); // a home on its own
    expect(pinAt(marks, { x: 90, y: 90 }, at, 5)).toBe(-1);
  });
});
