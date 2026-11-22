import { describe, expect, it } from 'vitest';
import { groupEffects } from './palette';
import type { EffectInfo } from './types';

const fx = (id: string): EffectInfo => ({
  id,
  version: 1,
  audacity: '',
  summary: '',
  details: '',
  random_within: false,
  exact_preview: false,
  params: [],
});

describe('groupEffects', () => {
  it('splits audio from data, in picker order', () => {
    const ids = ['shuffle', 'echo', 'invert', 'stutter', 'reverb', 'shift', 'delay', 'amplify', 'bitcrush', 'backwards', 'speed', 'phaser'];
    const groups = groupEffects(ids.map(fx));
    expect(groups.map((g) => g.label)).toEqual(['Audio', 'Data']);
    expect(groups[0].effects.map((e) => e.id)).toEqual(['echo', 'delay', 'reverb', 'phaser', 'amplify', 'speed', 'stutter']);
    expect(groups[1].effects.map((e) => e.id)).toEqual(['invert', 'bitcrush', 'backwards', 'shift', 'shuffle']);
  });

  it('puts unknown effects at the end of audio and drops empty groups', () => {
    const groups = groupEffects([fx('flanger'), fx('echo')]);
    expect(groups).toHaveLength(1);
    expect(groups[0].effects.map((e) => e.id)).toEqual(['echo', 'flanger']);
  });
});
