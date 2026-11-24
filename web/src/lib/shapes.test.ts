import { describe, expect, it } from 'vitest';
import {
  dragRect,
  grabPath,
  grabRect,
  insertPoint,
  insidePath,
  moveAll,
  moveHandle,
  niceAngle,
  onSide,
  pathPin,
  pullHandle,
  rectGeom,
  sides,
  toggleSmooth,
  type RectOp,
} from './shapes';

// A 400 x 200 picture on screen: the longer side is the width.
const W = 400;
const H = 200;
const close = (a: number, b: number, digits = 3) => expect(a).toBeCloseTo(b, digits);
const rect = (angle = 0): RectOp => ({ shape: 'rect', mode: 'add', center: [0.5, 0.5], size: [0.5, 0.25], angle });

describe('rectangles', () => {
  it('lays out corners and the turn handle', () => {
    // 200 x 100 px, centred at (200, 100).
    const g = rectGeom(rect(), W, H);
    expect(g.corners.map((c) => c.map(Math.round))).toEqual([
      [100, 50],
      [300, 50],
      [300, 150],
      [100, 150],
    ]);
    close(g.rotate[1], 50 - 30);
    // Turned a quarter clockwise, the top edge faces right.
    const q = rectGeom(rect(90), W, H);
    close(q.top[0], 200 + 50);
    close(q.top[1], 100);
  });

  it('grabs corners, the turn handle, the inside, or draws', () => {
    const r = rect();
    expect(grabRect(r, [102, 52], W, H, 10)).toEqual({ kind: 'corner', k: 0 });
    expect(grabRect(r, [200, 21], W, H, 10)).toEqual({ kind: 'rotate' });
    expect(grabRect(r, [250, 120], W, H, 10)).toEqual({ kind: 'move' });
    expect(grabRect(r, [20, 20], W, H, 10)).toEqual({ kind: 'draw' });
    // Turned 90: what was inside the old width is outside now.
    expect(grabRect(rect(90), [290, 100], W, H, 10)).toEqual({ kind: 'draw' });
  });

  it('resizes against the opposite corner, moves, turns and redraws', () => {
    const r = rect();
    // Pull the bottom-right corner from (300, 150) to (340, 190).
    const big = dragRect({ kind: 'corner', k: 2 }, r, [300, 150], [340, 190], W, H);
    expect(big.size).toEqual([0.6, 0.35]);
    expect(big.center).toEqual([0.55, 0.6]);
    const moved = dragRect({ kind: 'move' }, r, [200, 100], [240, 80], W, H);
    expect(moved.center).toEqual([0.6, 0.4]);
    expect(moved.size).toEqual(r.size);
    // The turn handle dragged to the right of the centre: 90 degrees.
    expect(dragRect({ kind: 'rotate' }, r, [200, 20], [300, 100], W, H).angle).toBe(90);
    // Drawing again keeps the angle and spans the drag.
    const drawn = dragRect({ kind: 'draw' }, rect(), [0, 0], [100, 50], W, H);
    expect(drawn).toEqual({ center: [0.125, 0.125], size: [0.25, 0.125], angle: 0 });
  });

  it('keeps angles tidy', () => {
    expect(niceAngle(91.2, false)).toBe(90);
    expect(niceAngle(97.3, false)).toBe(97.3);
    expect(niceAngle(97.3, true)).toBe(90);
    expect(niceAngle(-181, false)).toBe(180);
    expect(niceAngle(270, false)).toBe(-90);
    expect(niceAngle(0.4, false)).toBe(0);
  });
});

describe('pen paths', () => {
  // A square of corners, 100 px to 300 px across, 50 to 150 down.
  const square = [
    [0.25, 0.25],
    [0.75, 0.25],
    [0.75, 0.75],
    [0.25, 0.75],
  ];

  it('knows its inside, closing the last side', () => {
    expect(insidePath(square, [200, 100], W, H)).toBe(true);
    expect(insidePath(square, [50, 100], W, H)).toBe(false);
    // Three points: a triangle.
    expect(insidePath(square.slice(0, 3), [280, 60], W, H)).toBe(true);
    expect(insidePath(square.slice(0, 3), [120, 140], W, H)).toBe(false);
  });

  it('grabs points, sides, the inside, and the picked point handles', () => {
    expect(grabPath(square, [102, 51], W, H, 10, null)).toEqual({ kind: 'point', i: 0 });
    const side = grabPath(square, [200, 52], W, H, 10, null);
    expect(side.kind === 'side' && side.i).toBe(0);
    expect(grabPath(square, [200, 100], W, H, 10, null)).toEqual({ kind: 'move' });
    expect(grabPath(square, [20, 20], W, H, 10, null)).toEqual({ kind: 'none' });
    const smooth = toggleSmooth(square, 1, W, H);
    const out = [smooth[1][2] * W, smooth[1][3] * H] as [number, number];
    expect(grabPath(smooth, out, W, H, 10, 1)).toEqual({ kind: 'handle', i: 1, end: 'out' });
    // Not picked: its handles are hidden, so not grabbed.
    expect(grabPath(smooth, out, W, H, 10, null).kind).not.toBe('handle');
  });

  it('pulls a handle while placing, mirrored on the other side', () => {
    const pts = pullHandle(square, 1, [340, 50], W, H);
    expect(pts[1]).toEqual([0.75, 0.25, 0.85, 0.25]);
    // The side into it now arrives along the mirror: from the left.
    const s = sides(pts, W, H)[0];
    expect(s.curved).toBe(true);
    expect(s.c).toEqual([260, 50]);
    // A short drag leaves a corner.
    expect(pullHandle(square, 1, [302, 51], W, H)[1]).toEqual([0.75, 0.25]);
    // Dragging the mirrored end sets the stored one opposite.
    expect(moveHandle(pts, 1, 'in', [280, 50], W, H)[1]).toEqual([0.75, 0.25, 0.8, 0.25]);
  });

  it('adds a point on a side without changing its shape', () => {
    const straight = insertPoint(square, 0, 0.5, W, H);
    expect(straight).toHaveLength(5);
    expect(straight[1]).toEqual([0.5, 0.25]);
    // On a curve, the new point lies on it and is smooth.
    const curved = pullHandle(square, 0, [150, 20], W, H);
    const before = onSide(sides(curved, W, H)[0], 0.5);
    const after = insertPoint(curved, 0, 0.5, W, H);
    expect(after[1]).toHaveLength(4);
    close(after[1][0] * W, before[0], 1);
    close(after[1][1] * H, before[1], 1);
  });

  it('moves, smooths and pins', () => {
    expect(moveAll(square, 40, -20, W, H)[0]).toEqual([0.35, 0.15]);
    const s = toggleSmooth(square, 0, W, H);
    expect(s[0]).toHaveLength(4);
    expect(toggleSmooth(s, 0, W, H)[0]).toEqual([0.25, 0.25]);
    expect(pathPin(square, W, H)).toEqual([200, 100]);
    expect(pathPin([], W, H)).toBeNull();
  });
});
