import { describe, expect, it } from 'vitest';
import { DEFAULT_PULL_TOOL, PULL_MAX, PULL_MIN, cleanPullTool, moves, pullSizeScale, stepSize, strokeCount } from './pull';
import { SLIDER_STEPS } from './slider';

describe('pull tool', () => {
  it('falls back to defaults for missing or broken settings', () => {
    expect(cleanPullTool(null)).toEqual(DEFAULT_PULL_TOOL);
    expect(cleanPullTool({ size: 'big', strength: Number.NaN })).toEqual(DEFAULT_PULL_TOOL);
    expect(cleanPullTool({ size: 5, strength: -1 })).toEqual({ size: PULL_MAX, strength: 0.05 });
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

  it('counts strokes in words', () => {
    expect(strokeCount(0)).toBe('No strokes yet');
    expect(strokeCount(1)).toBe('1 stroke');
    expect(strokeCount(3)).toBe('3 strokes');
  });
});
