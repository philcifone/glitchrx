import { describe, expect, it } from 'vitest';
import { CROP_RATIOS, cropRect, fitSize, ratioLabel, ratioValue } from './canvas';

describe('canvas size', () => {
  it('fits like the server', () => {
    expect(fitSize(6240, 4160, 2048, 2048)).toEqual([2048, 1365]);
    expect(fitSize(4160, 6240, 1024, 1024)).toEqual([682, 1024]);
    expect(fitSize(600, 400, 4096, 4096)).toEqual([600, 400]);
    // The core's own test case: 480x320 into 200x200.
    expect(fitSize(480, 320, 200, 200)).toEqual([200, 133]);
  });

  it('crops like the server', () => {
    expect(cropRect({ x: 0.1, y: 0.25, w: 0.5, h: 0.5 }, 480, 320)).toEqual({ x: 48, y: 80, w: 240, h: 160 });
    expect(cropRect({ x: 0.999, y: 0, w: 0.0001, h: 1 }, 100, 10)).toEqual({ x: 99, y: 0, w: 1, h: 10 });
    expect(cropRect(undefined, 30, 20)).toEqual({ x: 0, y: 0, w: 30, h: 20 });
  });

  it('turns ratios with the orientation', () => {
    const r54 = CROP_RATIOS.find((c) => c.id === '5:4')!.sides;
    expect(ratioLabel(r54, true)).toBe('4:5');
    expect(ratioLabel(r54, false)).toBe('5:4');
    expect(ratioValue(r54, true, 1.5)).toBeCloseTo(0.8);
    expect(ratioValue([0, 0], true, 1.5)).toBe(1.5);
    expect(ratioValue(null, false, 1.5)).toBeNull();
  });
});
