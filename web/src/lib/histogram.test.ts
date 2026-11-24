import { describe, expect, it } from 'vitest';
import { countsFor, gammaToShare, histogramPath, shareToGamma } from './histogram';

describe('histogram drawing', () => {
  it('picks the channel, luma for all three', () => {
    const h = { red: [1], green: [2], blue: [3], luma: [4] };
    expect(countsFor(h, 'red')).toEqual([1]);
    expect(countsFor(h, 'rgb')).toEqual([4]);
  });

  it('scales to the tallest level away from the clipped ends', () => {
    const counts = new Array(256).fill(0);
    counts[0] = 1000; // clipped blacks
    counts[100] = 10;
    const d = histogramPath(counts, 100, 1);
    expect(d).toContain('L100,0.0'); // the tallest real level reaches the top
    expect(d).toContain('L0,0.0'); // the pile at 0 is cut off at the top
    expect(d.startsWith('M0,100')).toBe(true);
  });
});

describe('levels midtone handle', () => {
  it('sits in the middle at gamma 1, left of it when lighter', () => {
    expect(gammaToShare(1)).toBe(0.5);
    expect(gammaToShare(2)).toBe(0.25);
  });

  it('round trips and stays in range', () => {
    for (const g of [0.1, 0.5, 1, 1.7, 3, 10]) {
      expect(shareToGamma(gammaToShare(g))).toBeCloseTo(g, 2);
    }
    expect(shareToGamma(0)).toBe(10);
    expect(shareToGamma(1)).toBe(0.1);
  });
});
