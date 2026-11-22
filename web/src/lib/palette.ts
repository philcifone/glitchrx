// How the effect picker groups effects.
//
// "Audio" effects behave like their Audacity namesakes: they listen to the
// samples and add to them (repeats, ghosts, gain). "Data" effects treat the
// bytes as data: flipping, crushing, reversing, moving them around. The
// split is for finding things, not a technical one; stutter is an edit
// underneath but sounds like audio, so it sits with audio.

import type { EffectInfo } from './types';

export interface Group {
  label: string;
  effects: EffectInfo[];
}

const AUDIO = ['echo', 'delay', 'reverb', 'phaser', 'wahwah', 'amplify', 'distort', 'speed', 'stutter'];
const DATA = ['invert', 'bitcrush', 'backwards', 'shift', 'shuffle'];

/**
 * The effects in picker order, by group. Effects not listed (new ones) go
 * under audio, since the planned additions are Audacity filters.
 */
export function groupEffects(effects: EffectInfo[]): Group[] {
  const rank = (list: string[], id: string) => {
    const i = list.indexOf(id);
    return i < 0 ? list.length : i;
  };
  const data = effects.filter((e) => DATA.includes(e.id)).sort((a, b) => rank(DATA, a.id) - rank(DATA, b.id));
  const audio = effects.filter((e) => !DATA.includes(e.id)).sort((a, b) => rank(AUDIO, a.id) - rank(AUDIO, b.id));
  return [
    { label: 'Audio', effects: audio },
    { label: 'Data', effects: data },
  ].filter((g) => g.effects.length > 0);
}
