import { describe, expect, it } from 'vitest';
import { blockAt, isModified, moveGroup, moveStep, presetForm, stable, stackForm, tidy } from './groups';
import type { PresetGroup, StepState } from './types';

// Echo reads samples; pixelsort does not.
const samples = (e: string) => e !== 'pixelsort';
const step = (uid: string, group?: string, effect = 'echo'): StepState => ({
  uid,
  effect,
  seed: '1',
  params: {},
  enabled: true,
  ...(group ? { group } : {}),
});
const uids = (s: StepState[] | null) => s?.map((x) => x.uid).join('');

describe('preset groups', () => {
  // a, then group G (b c), then d, then group H (e).
  const stack = [step('a'), step('b', 'G'), step('c', 'G'), step('d'), step('e', 'H')];

  it('finds the block a step moves with', () => {
    expect(blockAt(stack, 0)).toEqual([0, 1]);
    expect(blockAt(stack, 2)).toEqual([1, 3]);
  });

  it('moves loose steps over whole groups, grouped ones only inside', () => {
    expect(uids(moveStep(stack, 'a', 1))).toBe('bcade');
    expect(uids(moveStep(stack, 'd', -1))).toBe('adbce');
    expect(uids(moveStep(stack, 'd', 1))).toBe('abced');
    expect(uids(moveStep(stack, 'b', 1))).toBe('acbde');
    expect(moveStep(stack, 'b', -1)).toBeNull();
    expect(moveStep(stack, 'c', 1)).toBeNull();
    expect(moveStep(stack, 'e', 1)).toBeNull();
  });

  it('moves a group as a block', () => {
    expect(uids(moveGroup(stack, 'G', -1))).toBe('bcade');
    expect(uids(moveGroup(stack, 'G', 1))).toBe('adbce');
    expect(uids(moveGroup(stack, 'H', -1))).toBe('abced');
    expect(moveGroup(stack, 'H', 1)).toBeNull();
    // Past another whole group in one move.
    expect(uids(moveGroup(moveGroup(stack, 'H', -1)!, 'H', -1))).toBe('aebcd');
  });

  it('keeps groups whole and drops empty ones', () => {
    const g = (id: string): PresetGroup => ({ id, preset: id, encoding: 'u8', channels: 1, base: [] });
    const t = tidy([step('a', 'G'), step('b'), step('c', 'G'), step('d', 'X')], [g('G'), g('H')]);
    expect(t.steps.map((s) => s.group ?? '-').join('')).toBe('G---');
    expect(t.groups.map((x) => x.id)).toEqual(['G']);
  });

  it('writes steps as the preset would hold them, and back', () => {
    const preset = { encoding: 'ulaw', channels: 1 as const };
    const here = { encoding: 'u8', channels: 3 as const };
    // Added to a u8 RGB recipe, an echo from a ulaw mono preset is pinned...
    const added = stackForm({ effect: 'echo', seed: '1', params: { decay: '0.5' } }, preset, here, samples);
    expect(added.params).toEqual({ encoding: 'ulaw', channels: '1', decay: '0.5' });
    // ...and the pins drop again on the way back.
    expect(presetForm(added, preset, here, samples).params).toEqual({ decay: '0.5' });
    // A step following the recipe, saved to the preset, spells it out.
    expect(presetForm({ effect: 'echo', seed: '1', params: {} }, preset, here, samples).params).toEqual({
      encoding: 'u8',
      channels: '3',
    });
    // Pixel effects have no samples to pin.
    expect(stackForm({ effect: 'pixelsort', seed: '1', params: {} }, preset, here, samples).params).toEqual({});
  });

  it('knows when a group has been edited, and when it is back', () => {
    const recipe = { encoding: 'u8', channels: 1 as const };
    const g: PresetGroup = {
      id: 'G',
      preset: 'moody',
      ...recipe,
      base: [{ effect: 'echo', seed: '1', params: { decay: '0.5', mix: '1' } }],
    };
    const s: StepState = { uid: 'a', group: 'G', effect: 'echo', seed: '1', params: { mix: '1', decay: '0.5' }, enabled: true };
    expect(isModified(g, [s], recipe, samples)).toBe(false);
    expect(isModified(g, [{ ...s, seed: '2' }], recipe, samples)).toBe(true);
    expect(isModified(g, [{ ...s, enabled: false }], recipe, samples)).toBe(true);
    // A new recipe-wide encoding changes what the steps do.
    expect(isModified(g, [s], { ...recipe, encoding: 'alaw' }, samples)).toBe(true);
    expect(stable({ b: 1, a: [{ d: 2, c: undefined }] })).toBe('{"a":[{"d":2}],"b":1}');
  });
});
