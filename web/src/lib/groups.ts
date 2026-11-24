// Preset groups (SPEC 4e, D-46). Steps added from a preset stay together
// in the stack under its name. The group remembers the preset's steps as
// they were added (its base), so it can tell when they have been edited
// ("modified"), put them back (Revert) or write the edits to the preset
// (Update preset).
//
// "Modified" is never stored: it is worked out by comparing the group's
// steps with its base, so undo, redo and editing a value back all give the
// right answer without bookkeeping.
//
// A group's steps are always next to each other. Moves keep it that way:
// a grouped step moves only inside its group, a loose step hops over a
// whole group, and a group moves as a block.

import type { Channels, PresetGroup, StepDto, StepState } from './types';

/** Step parameter keys that override the recipe's encoding and channels. */
export const ENCODING_KEY = 'encoding';
export const CHANNELS_KEY = 'channels';

/** The recipe-wide sample settings a step falls back on. */
export interface Samples {
  encoding: string;
  channels: Channels;
}

/** Whether an effect reads samples (and so has an encoding to pin). */
export type UsesSamples = (effect: string) => boolean;

/**
 * A step as its preset would hold it, in a recipe with the group's sample
 * settings: what the step does in `recipe` is spelled out in its own
 * keys, and keys that only repeat the group's settings are dropped.
 */
export function presetForm(s: StepDto, group: Samples, recipe: Samples, samples: UsesSamples): StepDto {
  const params = { ...s.params };
  if (samples(s.effect)) {
    const enc = params[ENCODING_KEY] ?? recipe.encoding;
    const ch = params[CHANNELS_KEY] ?? String(recipe.channels);
    if (enc === group.encoding) delete params[ENCODING_KEY];
    else params[ENCODING_KEY] = enc;
    if (ch === String(group.channels)) delete params[CHANNELS_KEY];
    else params[CHANNELS_KEY] = ch;
  }
  return {
    effect: s.effect,
    ...(s.effect_version === undefined ? {} : { effect_version: s.effect_version }),
    seed: s.seed,
    params,
    ...(s.mask ? { mask: s.mask } : {}),
  };
}

/** The inverse: a preset's step as it goes into a recipe with other settings. */
export function stackForm(s: StepDto, group: Samples, recipe: Samples, samples: UsesSamples): StepDto {
  if (!samples(s.effect)) return { ...s, params: { ...s.params } };
  return {
    ...s,
    params: {
      ...(group.encoding !== recipe.encoding ? { [ENCODING_KEY]: group.encoding } : {}),
      ...(group.channels !== recipe.channels ? { [CHANNELS_KEY]: String(group.channels) } : {}),
      ...s.params,
    },
  };
}

/** JSON with object keys sorted, so equal steps give equal text. */
export function stable(v: unknown): string {
  if (Array.isArray(v)) return `[${v.map(stable).join(',')}]`;
  if (v && typeof v === 'object') {
    const o = v as Record<string, unknown>;
    return `{${Object.keys(o)
      .filter((k) => o[k] !== undefined)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${stable(o[k])}`)
      .join(',')}}`;
  }
  return JSON.stringify(v);
}

/** The group's steps that run, in preset form. */
export function currentForm(g: PresetGroup, steps: StepState[], recipe: Samples, samples: UsesSamples): StepDto[] {
  return steps.filter((s) => s.group === g.id && s.enabled).map((s) => presetForm(s, g, recipe, samples));
}

/** Whether the group's steps differ from the preset's (turning one off counts). */
export function isModified(g: PresetGroup, steps: StepState[], recipe: Samples, samples: UsesSamples): boolean {
  return stable(currentForm(g, steps, recipe, samples)) !== stable(g.base);
}

/** The run of steps [start, end) that moves with the step at `i`: its group, or itself. */
export function blockAt(steps: StepState[], i: number): [number, number] {
  const g = steps[i]?.group;
  if (!g) return [i, i + 1];
  let a = i;
  let b = i + 1;
  while (a > 0 && steps[a - 1].group === g) a--;
  while (b < steps.length && steps[b].group === g) b++;
  return [a, b];
}

/** Move the block [a, b) past the neighbouring block on side `by`. Null if it cannot. */
function moveBlock<T extends StepState>(steps: T[], a: number, b: number, by: -1 | 1): T[] | null {
  if (by < 0 ? a === 0 : b === steps.length) return null;
  const [na, nb] = blockAt(steps, by < 0 ? a - 1 : b);
  const lo = Math.min(a, na);
  const hi = Math.max(b, nb);
  const mine = steps.slice(a, b);
  const theirs = steps.slice(na, nb);
  return [...steps.slice(0, lo), ...(by < 0 ? [...mine, ...theirs] : [...theirs, ...mine]), ...steps.slice(hi)];
}

/** Move one step up or down. Null if it cannot (the end of the stack or of its group). */
export function moveStep<T extends StepState>(steps: T[], uid: string, by: -1 | 1): T[] | null {
  const i = steps.findIndex((s) => s.uid === uid);
  if (i < 0) return null;
  if (steps[i].group) {
    const j = i + by;
    if (steps[j]?.group !== steps[i].group) return null;
    const out = [...steps];
    [out[i], out[j]] = [out[j], out[i]];
    return out;
  }
  return moveBlock(steps, i, i + 1, by);
}

/** Move a whole group up or down past its neighbour. */
export function moveGroup<T extends StepState>(steps: T[], id: string, by: -1 | 1): T[] | null {
  const i = steps.findIndex((s) => s.group === id);
  if (i < 0) return null;
  const [a, b] = blockAt(steps, i);
  return moveBlock(steps, a, b, by);
}

/**
 * Keep groups and steps consistent: a step's group must exist, a group
 * must have steps, and a group's steps must be together (any stragglers
 * after the first run leave it). Returns what to keep.
 */
export function tidy<T extends StepState>(
  steps: T[],
  groups: PresetGroup[] | undefined,
): { steps: T[]; groups: PresetGroup[] } {
  const known = new Set((groups ?? []).map((g) => g.id));
  const done = new Set<string>();
  let prev: string | undefined;
  const out = steps.map((s) => {
    let g = s.group;
    if (g && (!known.has(g) || (done.has(g) && prev !== g))) g = undefined;
    if (prev && prev !== g) done.add(prev);
    prev = g;
    if (g === s.group) return s;
    const { group: _drop, ...rest } = s;
    return rest as T;
  });
  const used = new Set(out.map((s) => s.group).filter(Boolean));
  return { steps: out, groups: (groups ?? []).filter((g) => used.has(g.id)) };
}

/** Groups read back from storage: only well-formed ones. */
export function cleanGroups(v: unknown): PresetGroup[] {
  if (!Array.isArray(v)) return [];
  return v.filter(
    (g): g is PresetGroup =>
      !!g &&
      typeof g.id === 'string' &&
      typeof g.preset === 'string' &&
      typeof g.encoding === 'string' &&
      (g.channels === 1 || g.channels === 2 || g.channels === 3) &&
      Array.isArray(g.base),
  );
}

/** The stack as the GUI lays it out: loose steps, and groups holding theirs. */
export type Block =
  | { kind: 'step'; step: StepState; i: number }
  | { kind: 'group'; group: PresetGroup; steps: { step: StepState; i: number }[] };

export function blocksOf(steps: StepState[], groups: PresetGroup[] | undefined): Block[] {
  const byId = new Map((groups ?? []).map((g) => [g.id, g]));
  const out: Block[] = [];
  steps.forEach((step, i) => {
    const last = out[out.length - 1];
    const g = step.group ? byId.get(step.group) : undefined;
    if (!g) out.push({ kind: 'step', step, i });
    else if (last?.kind === 'group' && last.group.id === g.id) last.steps.push({ step, i });
    else out.push({ kind: 'group', group: g, steps: [{ step, i }] });
  });
  return out;
}
