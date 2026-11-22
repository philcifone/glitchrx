// When a step's seed matters.
//
// The seed places a step's hits, and shuffle also uses it inside each
// window. A step with `hits=0` runs once over its whole region, so unless
// the effect draws its own random numbers, a new seed renders the same
// image. The GUI disables the re-roll button then, instead of letting it
// look broken.

import type { EffectInfo } from './types';

export function usesSeed(params: Record<string, string>, info: EffectInfo | undefined): boolean {
  if (!info) return false;
  if (info.random_within) return true;
  const hits = params.hits ?? info.params.find((p) => p.name === 'hits')?.default ?? '0';
  return Number(hits) > 0;
}
