import { describe, expect, it } from 'vitest';
import { usesSeed } from './seed';
import type { EffectInfo } from './types';

const effect = (hits: string, random_within = false): EffectInfo => ({
  id: 'x',
  version: 1,
  audacity: 'X',
  summary: '',
  details: '',
  random_within,
  params: [
    { name: 'hits', kind: 'int', min: 0, max: 1000, default: hits, help: '', placement: true },
  ],
});

describe('usesSeed', () => {
  it('follows the default hits', () => {
    expect(usesSeed({}, effect('0'))).toBe(false);
    expect(usesSeed({}, effect('4'))).toBe(true);
  });
  it('follows an overridden hits', () => {
    expect(usesSeed({ hits: '3' }, effect('0'))).toBe(true);
    expect(usesSeed({ hits: '0' }, effect('4'))).toBe(false);
  });
  it('is always true for effects that draw inside windows', () => {
    expect(usesSeed({ hits: '0' }, effect('0', true))).toBe(true);
  });
  it('is false for unknown effects', () => {
    expect(usesSeed({}, undefined)).toBe(false);
  });
});
