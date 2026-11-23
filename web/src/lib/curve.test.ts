import { describe, expect, it } from 'vitest';
import { curveFn, formatCurve, parseCurve } from './curve';

describe('curves', () => {
  it('reads and writes the core text form', () => {
    expect(parseCurve('0:0/64:52/192:204/255:255')).toEqual([[0, 0], [64, 52], [192, 204], [255, 255]]);
    expect(formatCurve([[0, 10], [255, 200.4]])).toBe('0:10/255:200');
    for (const bad of ['0:0', '0:0/0:5', '10:0/5:5', '0:0/256:1', '0:0,255:255', '']) {
      expect(parseCurve(bad)).toBeNull();
    }
  });

  it('passes through its points, rises with them, and is flat outside', () => {
    const p = parseCurve('20:10/50:150/60:155/230:240')!;
    const f = curveFn(p);
    for (const [x, y] of p) expect(f(x)).toBeCloseTo(y, 9);
    let last = -1;
    for (let x = 0; x <= 255; x += 0.5) {
      expect(f(x)).toBeGreaterThanOrEqual(last - 1e-9);
      last = f(x);
    }
    expect(f(0)).toBe(10);
    expect(f(255)).toBe(240);
  });
});
