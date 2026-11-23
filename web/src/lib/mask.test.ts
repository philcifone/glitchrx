import { describe, expect, it } from 'vitest';
import {
  photoFrame,
  photoRect,
  toPhoto,
  addPoint,
  afterRemoval,
  brushScale,
  cleanTool,
  dragEnds,
  featherScale,
  grab,
  grabHandle,
  insertOp,
  migrateMask,
  newComponent,
  opDetail,
  pasteOps,
  percent,
  withMode,
} from './mask';
import type { MaskMode, MaskOp } from './types';

const brush = (mode: MaskMode = 'add'): MaskOp => ({ shape: 'brush', mode, stroke: [] });
const bright = (mode: MaskMode): MaskOp => ({ shape: 'brightness', mode, low: 0.5, high: 1, soft: 0.1 });
const modes = (ops: MaskOp[]) => ops.map((o) => `${o.shape}:${o.mode}`);

describe('insertOp', () => {
  it('keeps intersections last, so they limit everything else', () => {
    let ops: MaskOp[] = [];
    ops = insertOp(ops, brush()).ops;
    ops = insertOp(ops, bright('intersect')).ops;
    const r = insertOp(ops, brush('subtract'));
    expect(modes(r.ops)).toEqual(['brush:add', 'brush:subtract', 'brightness:intersect']);
    expect(r.index).toBe(1);
  });
});

describe('withMode', () => {
  it('moves a new intersection to the end and back out again', () => {
    let ops = [bright('add'), brush(), brush()];
    let r = withMode(ops, 0, 'intersect');
    expect(modes(r.ops)).toEqual(['brush:add', 'brush:add', 'brightness:intersect']);
    expect(r.order).toEqual([1, 2, 0]);
    ops = r.ops;
    r = withMode(ops, 2, 'subtract');
    expect(modes(r.ops)).toEqual(['brush:add', 'brush:add', 'brightness:subtract']);
    r = withMode(r.ops, 0, 'subtract');
    expect(modes(r.ops)).toEqual(['brush:subtract', 'brush:add', 'brightness:subtract']);
    expect(r.order).toEqual([0, 1, 2]);
  });

  it('reports where a component leaving the intersect group lands', () => {
    const ops = [brush(), bright('intersect'), bright('intersect')];
    const r = withMode(ops, 2, 'add');
    expect(modes(r.ops)).toEqual(['brush:add', 'brightness:add', 'brightness:intersect']);
    // The selection follows: old index 2 is now at 1.
    expect(r.order.indexOf(2)).toBe(1);
  });
});

describe('afterRemoval', () => {
  it('follows the selection or drops it', () => {
    expect(afterRemoval(3, [1])).toBe(2);
    expect(afterRemoval(1, [3])).toBe(1);
    expect(afterRemoval(1, [1])).toBeNull();
    expect(afterRemoval(null, [0])).toBeNull();
  });
});

describe('migrateMask', () => {
  it('folds runs of old stroke operations into brushes', () => {
    const old = {
      invert: true,
      feather: 0.01,
      op: [
        { shape: 'stroke', mode: 'add', size: 0.05, hardness: 0.4, points: [0.1, 0.1] },
        { shape: 'stroke', mode: 'subtract', size: 0.02, points: [0.2, 0.2] },
        { shape: 'radial', mode: 'add', from: [0.5, 0.5], to: [0.6, 0.5] },
        { shape: 'stroke', mode: 'add', size: 0.05, hardness: 0.5, points: [0.3, 0.3] },
      ],
    } as never;
    const m = migrateMask(old);
    expect(m.invert).toBe(true);
    // The mask-wide feather moves onto every component.
    expect('feather' in m).toBe(false);
    expect(m.op!.map((o) => o.feather)).toEqual([0.01, 0.01, 0.01]);
    for (const o of m.op!) delete o.feather;
    expect(modes(m.op!)).toEqual(['brush:add', 'radial:add', 'brush:add']);
    expect(m.op![0]).toEqual({
      shape: 'brush',
      mode: 'add',
      stroke: [
        { size: 0.05, hardness: 0.4, points: [0.1, 0.1] },
        { size: 0.02, hardness: 0.5, points: [0.2, 0.2], erase: true },
      ],
    });
  });

  it('leaves a current mask alone', () => {
    const m = { invert: false, op: [brush(), { ...bright('intersect'), feather: 0.02 }] };
    expect(migrateMask(m)).toEqual(m);
    // A component's own feather wins over an old mask-wide one.
    const old = { ...m, feather: 0.05 } as never;
    expect(migrateMask(old).op!.map((o) => o.feather)).toEqual([0.05, 0.02]);
  });
});

describe('pasteOps', () => {
  it('keeps intersections last, copies deeply and reports where the pasted ones went', () => {
    const ops = [brush(), bright('intersect')];
    const copied = [bright('intersect'), { ...brush('subtract'), opacity: 0.5 }];
    const { ops: out, indices } = pasteOps(ops, copied);
    expect(modes(out)).toEqual(['brush:add', 'brush:subtract', 'brightness:intersect', 'brightness:intersect']);
    expect(indices).toEqual([1, 3]);
    expect(out[1].opacity).toBe(0.5);
    // Pasting twice gives independent copies.
    expect(out[3]).not.toBe(copied[0]);
  });
});

describe('newComponent', () => {
  it('starts a radial as a centred circle a fifth of the long side across in radius', () => {
    const wide = newComponent('radial', 1.5, 0) as Extract<MaskOp, { shape: 'radial' }>;
    expect(wide.from).toEqual([0.5, 0.5]);
    expect(wide.to).toEqual([0.7, 0.5]);
    expect(wide.hardness).toBe(1);
    // On a tall picture the long side is the height: radius 0.2 of it is
    // 0.3 of the width at 2:3.
    const tall = newComponent('radial', 2 / 3, 0) as Extract<MaskOp, { shape: 'radial' }>;
    expect(tall.to[0]).toBeCloseTo(0.8, 4);
  });

  it('makes a brightness range a limit once there is something to limit', () => {
    expect(newComponent('brightness', 1, 0).mode).toBe('add');
    expect(newComponent('brightness', 1, 2).mode).toBe('intersect');
  });
});

describe('opDetail', () => {
  it('counts a brush\'s strokes', () => {
    const s = { size: 0.05, hardness: 0.5, points: [0.5, 0.5] };
    expect(opDetail(brush())).toBe('no strokes yet');
    expect(opDetail({ shape: 'brush', mode: 'add', stroke: [s, s, { ...s, erase: true }] })).toBe('2 strokes, 1 erased');
    expect(opDetail(bright('add'))).toBe('50-100');
  });
});

describe('cleanTool', () => {
  it('replaces tools from before components', () => {
    expect(cleanTool({ tool: 'radial', size: 0.1 })).toMatchObject({ tool: 'paint', size: 0.1 });
    expect(cleanTool({ tool: 'erase' }).tool).toBe('erase');
    expect(cleanTool(null).tool).toBe('paint');
  });
});

describe('grab', () => {
  // A 200 x 100 display; the radial is centred at (100, 50), radius 30.
  const radial = { shape: 'radial', mode: 'add', from: [0.5, 0.5], to: [0.65, 0.5] } as Extract<MaskOp, { shape: 'radial' }>;
  const linear = { shape: 'linear', mode: 'add', from: [0.25, 0.5], to: [0.75, 0.5] } as Extract<MaskOp, { shape: 'linear' }>;

  it('finds a radial\'s ring, inside and outside', () => {
    expect(grab(radial, [130, 50], 200, 100, 8)).toBe('edge'); // the edge handle
    expect(grab(radial, [100, 21], 200, 100, 8)).toBe('edge'); // on the ring
    expect(grab(radial, [105, 55], 200, 100, 8)).toBe('move');
    expect(grab(radial, [10, 10], 200, 100, 8)).toBe('draw');
  });

  it('finds a linear\'s end lines and middle', () => {
    expect(grab(linear, [50, 50], 200, 100, 8)).toBe('from');
    expect(grab(linear, [52, 5], 200, 100, 8)).toBe('from'); // along the start line
    expect(grab(linear, [148, 90], 200, 100, 8)).toBe('to');
    expect(grab(linear, [100, 52], 200, 100, 8)).toBe('move');
    expect(grab(linear, [100, 90], 200, 100, 8)).toBe('draw');
  });
});

describe('grabHandle', () => {
  it('finds only the dots, the far one first', () => {
    const radial = { shape: 'radial', mode: 'add', from: [0.5, 0.5], to: [0.65, 0.5] } as Extract<MaskOp, { shape: 'radial' }>;
    expect(grabHandle(radial, [101, 50], 200, 100, 8)).toBe('move');
    expect(grabHandle(radial, [131, 50], 200, 100, 8)).toBe('edge');
    expect(grabHandle(radial, [115, 50], 200, 100, 8)).toBeNull(); // inside, not on a dot
    // A tiny circle: both dots in reach, and pulling it out wins.
    const tiny = { ...radial, to: [0.51, 0.5] as [number, number] };
    expect(grabHandle(tiny, [101, 50], 200, 100, 8)).toBe('edge');
  });
});

describe('dragEnds', () => {
  const from: [number, number] = [0.5, 0.5];
  const to: [number, number] = [0.6, 0.5];
  it('moves, resizes and redraws', () => {
    expect(dragEnds('move', from, to, [100, 50], [120, 60], 200, 100)).toEqual({ from: [0.6, 0.6], to: [0.7, 0.6] });
    expect(dragEnds('edge', from, to, [120, 50], [150, 50], 200, 100)).toEqual({ from, to: [0.75, 0.5] });
    expect(dragEnds('from', from, to, [100, 50], [20, 10], 200, 100)).toEqual({ from: [0.1, 0.1], to });
    expect(dragEnds('draw', from, to, [20, 10], [40, 10], 200, 100)).toEqual({ from: [0.1, 0.1], to: [0.2, 0.1] });
  });
});

describe('addPoint', () => {
  it('skips points closer than the minimum and rounds the rest', () => {
    const pts: number[] = [];
    expect(addPoint(pts, 0.1234567, 0.5, 4, 1000, 1000)).toBe(true);
    expect(addPoint(pts, 0.125, 0.5, 4, 1000, 1000)).toBe(false); // 1.5 px away
    expect(addPoint(pts, 0.13, 0.5, 4, 1000, 1000)).toBe(true);
    expect(pts).toEqual([0.12346, 0.5, 0.13, 0.5]);
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

describe('masks and the crop (D-52)', () => {
  // The right half of a 256 x 128 photo, as in the core's test.
  const f = photoFrame({ x: 128, y: 0, w: 128, h: 128 }, 256, 128);

  it('matches the core frame', () => {
    expect(f).toEqual({ x: 0.5, y: 0, sx: 2, sy: 1, sl: 2 });
  });

  it('places the whole photo around the canvas on screen', () => {
    // The canvas shown 200 px wide at x 50: the photo is twice as wide,
    // starting a canvas-width to the left.
    expect(photoRect({ x: 50, y: 10, w: 200, h: 200 }, f)).toEqual({ x: -150, y: 10, w: 400, h: 200 });
  });

  it('moves a new component from the canvas onto the photo', () => {
    const r = toPhoto({ shape: 'radial', mode: 'add', from: [0.5, 0.5], to: [0.7, 0.5], hardness: 1 }, f);
    expect(r).toMatchObject({ from: [0.75, 0.5], to: [0.85, 0.5] });
    const q = toPhoto({ shape: 'rect', mode: 'add', center: [0.5, 0.5], size: [0.5, 0.5] }, f);
    expect(q).toMatchObject({ center: [0.75, 0.5], size: [0.25, 0.25] });
  });
});
