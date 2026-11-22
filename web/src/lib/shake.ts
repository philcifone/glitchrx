// "Shake": random settings for one step.
//
// The dice re-rolls a step's seed, which only moves where things land. Shake
// rolls the effect's own parameters instead (delay, decay, phase, ...), each
// within the range its slider covers, skipping the extremes, which are
// rarely pretty. Placement and any encoding override are left alone, so a
// shake changes the sound, not where it plays. The values land in the
// recipe like hand-set ones, so a lucky shake can be saved and replayed.

import { SLIDER_STEPS, rowsScale, sliderScale } from './slider';
import type { EffectInfo } from './types';

/** Slider span a shake draws from: the middle 80%. */
const LO = 0.1;
const HI = 0.9;

/**
 * New values for every own (non-placement) parameter of `info`.
 * `rand` returns numbers in [0, 1), like Math.random.
 */
export function shakeParams(info: EffectInfo, rand: () => number): Record<string, string> {
  const slider = () => (LO + (HI - LO) * rand()) * SLIDER_STEPS;
  const out: Record<string, string> = {};
  for (const p of info.params) {
    if (p.placement) continue;
    switch (p.kind) {
      case 'int':
      case 'float': {
        const scale = sliderScale(p.min ?? 0, p.max ?? 1, p.kind === 'int');
        out[p.name] = String(scale.fromSlider(slider()));
        break;
      }
      case 'size':
        // Sizes are rolled in rows: fractions slide sideways, whole rows
        // move down, and both suit any photo size.
        out[p.name] = `${rowsScale.fromSlider(slider())}rows`;
        break;
      case 'choice': {
        const c = p.choices ?? [];
        if (c.length) out[p.name] = c[Math.floor(rand() * c.length)];
        break;
      }
      case 'region':
        break; // only placement has regions
    }
  }
  return out;
}
