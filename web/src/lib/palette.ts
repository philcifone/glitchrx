// How the effect picker groups effects.
//
// The server says which group each effect is in. "Audio" effects behave
// like their Audacity namesakes: they listen to the samples and add to them
// (repeats, ghosts, gain). "Data" effects treat the bytes as data: flipping,
// crushing, reversing, moving them around. "Pixel" effects are not audio at
// all: they work on whole pixels in two dimensions (pixel sorting).
// "Adjust" holds the photo tools (levels, curves, hue and saturation,
// color balance), steps like any other. Within a group, effects keep a
// fixed picker order, so related ones sit together.

import type { EffectGroup, EffectInfo } from './types';

export interface Group {
  label: string;
  effects: EffectInfo[];
}

const GROUPS: { id: EffectGroup; label: string }[] = [
  { id: 'audio', label: 'Audio' },
  { id: 'data', label: 'Data' },
  { id: 'pixel', label: 'Pixel' },
  { id: 'adjust', label: 'Adjust' },
];

/** Picker order inside each group; effects not listed go last. */
const ORDER = [
  'echo', 'delay', 'reverb', 'phaser', 'wahwah', 'amplify', 'distort', 'speed', 'stutter',
  'invert', 'bitcrush', 'backwards', 'shift', 'shuffle',
  'pixelsort',
  'levels', 'curves', 'hsl', 'balance',
];

/** The effects in picker order, by group; empty groups are left out. */
export function groupEffects(effects: EffectInfo[]): Group[] {
  const rank = (id: string) => {
    const i = ORDER.indexOf(id);
    return i < 0 ? ORDER.length : i;
  };
  return GROUPS.map((g) => ({
    label: g.label,
    effects: effects.filter((e) => (e.group ?? 'audio') === g.id).sort((a, b) => rank(a.id) - rank(b.id)),
  })).filter((g) => g.effects.length > 0);
}

/** Whether the step's encoding and channels apply to this effect. */
export function usesSamples(e: EffectInfo | undefined): boolean {
  return e?.group !== 'pixel' && e?.group !== 'adjust';
}

/**
 * Whether Shake suits the effect. Not for adjustments: random levels or
 * curves are rarely a happy accident, and black and white points drawn
 * independently can cross.
 */
export function shakes(e: EffectInfo | undefined): boolean {
  return e?.group !== 'adjust';
}
