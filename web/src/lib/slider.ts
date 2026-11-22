// Slider scales for parameters with very different ranges.
//
// `hits` runs 0..10000 and `delay` 1..16M, but the interesting values sit
// near the bottom. A linear slider would put all of them in the first
// pixel. Wide ranges therefore use a logarithmic scale: each slider step
// multiplies the value instead of adding to it. Ranges that cross zero
// (`shift by`, -16M..16M) use a *signed* log scale, mirrored around the
// middle of the slider, so small shifts either way are easy to reach.

export const SLIDER_STEPS = 1000;

export interface SliderScale {
  /** Slider position (0..SLIDER_STEPS) for a value. */
  toSlider(v: number): number;
  /** Value for a slider position. */
  fromSlider(t: number): number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Round to whole numbers (ints) or two decimals (floats). */
function rounder(isInt: boolean): (v: number) => number {
  return isInt ? Math.round : (v) => Math.round(v * 100) / 100;
}

export function sliderScale(min: number, max: number, isInt: boolean): SliderScale {
  const round = rounder(isInt);
  const span = max - min;
  const linear = isInt ? span <= 200 : span <= 10;

  if (linear) {
    return {
      toSlider: (v) => clamp(((v - min) / span) * SLIDER_STEPS, 0, SLIDER_STEPS),
      fromSlider: (t) => clamp(round(min + (t / SLIDER_STEPS) * span), min, max),
    };
  }

  if (min < 0 && max > 0) {
    // Signed log: the middle of the slider is 0.
    const half = SLIDER_STEPS / 2;
    const reach = Math.log1p(Math.max(-min, max));
    return {
      toSlider: (v) => {
        const u = Math.log1p(Math.abs(v)) / reach;
        return clamp(half + Math.sign(v) * u * half, 0, SLIDER_STEPS);
      },
      fromSlider: (t) => {
        const u = (t - half) / half;
        const v = Math.sign(u) * Math.expm1(Math.abs(u) * reach);
        return clamp(round(v), min, max);
      },
    };
  }

  // One-sided log, offset so `min` maps to the left end.
  const reach = Math.log1p(span);
  return {
    toSlider: (v) => clamp((Math.log1p(v - min) / reach) * SLIDER_STEPS, 0, SLIDER_STEPS),
    fromSlider: (t) => clamp(round(min + Math.expm1((t / SLIDER_STEPS) * reach)), min, max),
  };
}

/** Log scale for percentages, 0.01% .. 100%, used by size parameters. */
export const percentScale: SliderScale = (() => {
  const lo = Math.log(0.01);
  const hi = Math.log(100);
  const round = (v: number) => (v >= 10 ? Math.round(v) : v >= 1 ? Math.round(v * 10) / 10 : Math.round(v * 100) / 100);
  return {
    toSlider: (v) => clamp(((Math.log(Math.max(v, 0.01)) - lo) / (hi - lo)) * SLIDER_STEPS, 0, SLIDER_STEPS),
    fromSlider: (t) => clamp(round(Math.exp(lo + (t / SLIDER_STEPS) * (hi - lo))), 0.01, 100),
  };
})();
