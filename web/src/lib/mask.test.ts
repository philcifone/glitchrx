import { describe, expect, it } from 'vitest';
import { addPoint, brushScale, featherScale, insertOp, opRows, percent, withMode } from './mask';
import type { MaskMode, MaskOp } from './types';

const stroke = (mode: MaskMode = 'add'): MaskOp => ({ shape: 'stroke', mode, size: 0.05, hardness: 0.5, points: [0.5, 0.5] });
const bright = (mode: MaskMode): MaskOp => ({ shape: 'brightness', mode, low: 0.5, high: 1, soft: 0.1 });
const modes = (ops: MaskOp[]) => ops.map((o) => `${o.shape}:${o.mode}`);

describe('insertOp', () => {
  it('keeps intersections last, so they limit everything painted', () => {
    let ops: MaskOp[] = [];
    ops = insertOp(ops, stroke());
    ops = insertOp(ops, bright('intersect'));
    ops = insertOp(ops, stroke('subtract'));
    expect(modes(ops)).toEqual(['stroke:add', 'stroke:subtract', 'brightness:intersect']);
  });
});

describe('withMode', () => {
  it('moves a new intersection to the end and back out again', () => {
    let ops = [bright('add'), stroke(), stroke()];
    ops = withMode(ops, 0, 'intersect');
    expect(modes(ops)).toEqual(['stroke:add', 'stroke:add', 'brightness:intersect']);
    ops = withMode(ops, 2, 'subtract');
    expect(modes(ops)).toEqual(['stroke:add', 'stroke:add', 'brightness:subtract']);
    ops = withMode(ops, 0, 'subtract');
    expect(modes(ops)).toEqual(['stroke:subtract', 'stroke:add', 'brightness:subtract']);
  });
});

describe('opRows', () => {
  it('groups strokes and counts erasing', () => {
    const rows = opRows([stroke(), bright('add'), stroke('subtract'), stroke()]);
    expect(rows).toHaveLength(2);
    expect(rows[0]).toEqual({ kind: 'strokes', painted: 2, erased: 1, indices: [0, 2, 3] });
    expect(rows[1]).toMatchObject({ kind: 'op', index: 1 });
  });
});

describe('addPoint', () => {
  it('skips points closer than the minimum and rounds the rest', () => {
    const pts: number[] = [];
    expect(addPoint(pts, 0.123456, 0.5, 4, 1000, 1000)).toBe(true);
    expect(addPoint(pts, 0.125, 0.5, 4, 1000, 1000)).toBe(false); // 1.5 px away
    expect(addPoint(pts, 0.13, 0.5, 4, 1000, 1000)).toBe(true);
    expect(pts).toEqual([0.1235, 0.5, 0.13, 0.5]);
  });
});

describe('scales', () => {
  it('round-trip within their ranges', () => {
    for (const v of [0.005, 0.02, 0.06, 0.3]) expect(brushScale.fromSlider(brushScale.toSlider(v))).toBeCloseTo(v, 3);
    for (const v of [0, 0.004, 0.02, 0.1]) expect(featherScale.fromSlider(featherScale.toSlider(v))).toBeCloseTo(v, 4);
    expect(featherScale.toSlider(0.025)).toBe(500); // half way is a quarter of the range
  });

  it('format percentages', () => {
    expect(percent(0.0125)).toBe('1.3%');
    expect(percent(0.25)).toBe('25%');
  });
});
