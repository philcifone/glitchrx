import { describe, expect, it } from 'vitest';
import { groupEffects, usesSamples } from './palette';
import type { EffectGroup, EffectInfo } from './types';

const GROUP: Record<string, EffectGroup> = {
  invert: 'data',
  bitcrush: 'data',
  backwards: 'data',
  shift: 'data',
  shuffle: 'data',
  pixelsort: 'pixel',
};

const fx = (id: string): EffectInfo => ({
  id,
  group: GROUP[id] ?? 'audio',
  version: 1,
  audacity: '',
  summary: '',
  details: '',
  random_within: false,
  exact_preview: false,
  params: [],
});

describe('groupEffects', () => {
  it('splits audio, data and pixel, in picker order', () => {
    const ids = ['shuffle', 'pixelsort', 'echo', 'invert', 'stutter', 'reverb', 'shift', 'delay', 'amplify', 'bitcrush', 'backwards', 'speed', 'phaser'];
    const groups = groupEffects(ids.map(fx));
    expect(groups.map((g) => g.label)).toEqual(['Audio', 'Data', 'Pixel']);
    expect(groups[0].effects.map((e) => e.id)).toEqual(['echo', 'delay', 'reverb', 'phaser', 'amplify', 'speed', 'stutter']);
    expect(groups[1].effects.map((e) => e.id)).toEqual(['invert', 'bitcrush', 'backwards', 'shift', 'shuffle']);
    expect(groups[2].effects.map((e) => e.id)).toEqual(['pixelsort']);
  });

  it('puts unknown effects last in their group and drops empty groups', () => {
    const groups = groupEffects([fx('flanger'), fx('echo')]);
    expect(groups).toHaveLength(1);
    expect(groups[0].effects.map((e) => e.id)).toEqual(['echo', 'flanger']);
  });

  it('knows pixel effects ignore the sample settings', () => {
    expect(usesSamples(fx('pixelsort'))).toBe(false);
    expect(usesSamples(fx('echo'))).toBe(true);
  });
});
