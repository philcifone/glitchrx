import { describe, expect, it } from 'vitest';
import { shakeParams } from './shake';
import type { EffectInfo } from './types';

const echo: EffectInfo = {
  id: 'echo',
  version: 1,
  audacity: 'Echo',
  summary: '',
  details: '',
  random_within: false,
  params: [
    { name: 'delay', kind: 'size', default: '0.33rows', help: '', placement: false },
    { name: 'phase', kind: 'int', min: 0, max: 5, default: '1', help: '', placement: false },
    { name: 'decay', kind: 'float', min: 0, max: 1, default: '0.5', help: '', placement: false },
    { name: 'region', kind: 'region', default: '0%:100%', help: '', placement: true },
    { name: 'hits', kind: 'int', min: 0, max: 1000, default: '0', help: '', placement: true },
  ],
};

/** A repeatable stand-in for Math.random. */
function lcg(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2 ** 31;
    return s / 2 ** 31;
  };
}

describe('shakeParams', () => {
  it('rolls every own parameter and leaves placement alone', () => {
    const p = shakeParams(echo, lcg(1));
    expect(Object.keys(p).sort()).toEqual(['decay', 'delay', 'phase']);
  });

  it('stays inside the useful range', () => {
    const rand = lcg(7);
    for (let i = 0; i < 500; i++) {
      const p = shakeParams(echo, rand);
      const decay = Number(p.decay);
      expect(decay).toBeGreaterThanOrEqual(0.1);
      expect(decay).toBeLessThanOrEqual(0.9);
      const phase = Number(p.phase);
      expect(Number.isInteger(phase) && phase >= 0 && phase <= 5).toBe(true);
      const m = /^([\d.]+)rows$/.exec(p.delay);
      expect(m).not.toBeNull();
      expect(Number(m![1])).toBeGreaterThan(0);
    }
  });

  it('actually varies', () => {
    const rand = lcg(3);
    const seen = new Set(Array.from({ length: 20 }, () => shakeParams(echo, rand).delay));
    expect(seen.size).toBeGreaterThan(10);
  });
});
