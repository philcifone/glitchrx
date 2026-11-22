import { describe, expect, it } from 'vitest';
import { SLIDER_STEPS, percentScale, rowsScale, sliderScale } from './slider';

describe('sliderScale', () => {
  it('is linear for small integer ranges', () => {
    const s = sliderScale(1, 8, true);
    expect(s.fromSlider(0)).toBe(1);
    expect(s.fromSlider(SLIDER_STEPS)).toBe(8);
    expect(s.fromSlider(s.toSlider(5))).toBe(5);
  });

  it('is logarithmic for wide ranges, hitting both ends', () => {
    const s = sliderScale(1, 1 << 24, true);
    expect(s.fromSlider(0)).toBe(1);
    expect(s.fromSlider(SLIDER_STEPS)).toBe(1 << 24);
    // Half way is far below half the range.
    expect(s.fromSlider(SLIDER_STEPS / 2)).toBeLessThan(10_000);
    for (const v of [1, 7, 4096, 100_000]) {
      expect(Math.abs(s.fromSlider(s.toSlider(v)) - v) / v).toBeLessThan(0.02);
    }
  });

  it('is monotonic', () => {
    for (const s of [sliderScale(0, 10_000, true), sliderScale(-(1 << 24), 1 << 24, true)]) {
      let prev = -Infinity;
      for (let t = 0; t <= SLIDER_STEPS; t++) {
        const v = s.fromSlider(t);
        expect(v).toBeGreaterThanOrEqual(prev);
        prev = v;
      }
    }
  });

  it('centres signed ranges on zero', () => {
    const s = sliderScale(-(1 << 24), 1 << 24, true);
    expect(s.fromSlider(SLIDER_STEPS / 2)).toBe(0);
    expect(s.toSlider(0)).toBe(SLIDER_STEPS / 2);
    expect(s.fromSlider(0)).toBe(-(1 << 24));
    expect(s.fromSlider(s.toSlider(-200))).toBeCloseTo(-200, -1);
  });

  it('rounds floats to two decimals', () => {
    const s = sliderScale(0, 1, false);
    expect(s.fromSlider(333)).toBe(0.33);
  });
});

describe('percentScale', () => {
  it('covers 0.01% to 100%', () => {
    expect(percentScale.fromSlider(0)).toBe(0.01);
    expect(percentScale.fromSlider(SLIDER_STEPS)).toBe(100);
    expect(percentScale.fromSlider(percentScale.toSlider(0.5))).toBeCloseTo(0.5, 2);
  });
});

describe('rowsScale', () => {
  it('covers fractions of a row and many rows', () => {
    expect(rowsScale.fromSlider(0)).toBe(0.01);
    expect(rowsScale.fromSlider(SLIDER_STEPS)).toBe(64);
    // One row sits a bit past the middle: fractions get their share.
    const one = rowsScale.toSlider(1);
    expect(one).toBeGreaterThan(SLIDER_STEPS * 0.4);
    expect(one).toBeLessThan(SLIDER_STEPS * 0.7);
  });

  it('round-trips the defaults', () => {
    for (const v of [0.25, 0.33, 0.5, 1, 6.5]) {
      expect(rowsScale.fromSlider(rowsScale.toSlider(v))).toBeCloseTo(v, 1);
    }
  });
});
