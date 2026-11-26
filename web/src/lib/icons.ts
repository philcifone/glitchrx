// An icon for each effect, for the effect palette and step headers.
//
// Picked by Phil from a contact sheet of Lucide icons: playful rather than
// literal (a mountain for echo, a drum for stutter). The dice stays
// reserved for re-rolling seeds, and shuffle for shaking settings. Distort
// is a shell (Phil, 2026-11-22), for its spiralling warp.

import Mountain from '@lucide/svelte/icons/mountain';
import Copy from '@lucide/svelte/icons/copy';
import IterationCw from '@lucide/svelte/icons/iteration-cw';
import Megaphone from '@lucide/svelte/icons/megaphone';
import Eclipse from '@lucide/svelte/icons/eclipse';
import Blocks from '@lucide/svelte/icons/blocks';
import Rewind from '@lucide/svelte/icons/rewind';
import MoveHorizontal from '@lucide/svelte/icons/move-horizontal';
import Orbit from '@lucide/svelte/icons/orbit';
import Drum from '@lucide/svelte/icons/drum';
import Tornado from '@lucide/svelte/icons/tornado';
import Footprints from '@lucide/svelte/icons/footprints';
import Shell from '@lucide/svelte/icons/shell';
import Rabbit from '@lucide/svelte/icons/rabbit';
import ArrowDownWideNarrow from '@lucide/svelte/icons/arrow-down-wide-narrow';
import HandGrab from '@lucide/svelte/icons/hand-grab';
import Sparkles from '@lucide/svelte/icons/sparkles';
import ChartColumn from '@lucide/svelte/icons/chart-column';
import Spline from '@lucide/svelte/icons/spline';
import Palette from '@lucide/svelte/icons/palette';
import SunMoon from '@lucide/svelte/icons/sun-moon';
import Waves from '@lucide/svelte/icons/waves';
import Activity from '@lucide/svelte/icons/activity';
import AudioWaveform from '@lucide/svelte/icons/audio-waveform';
import Funnel from '@lucide/svelte/icons/funnel';
import SlidersVertical from '@lucide/svelte/icons/sliders-vertical';

export type IconComponent = typeof Mountain;

const ICONS: Record<string, IconComponent> = {
  echo: Mountain,
  delay: Copy,
  reverb: IterationCw,
  amplify: Megaphone,
  invert: Eclipse,
  bitcrush: Blocks,
  backwards: Rewind,
  shift: MoveHorizontal,
  shuffle: Orbit,
  stutter: Drum,
  // Second batch (2d): a swirl for the phaser, a foot pedal for the wah,
  // a spark for distortion, a rabbit for speed.
  phaser: Tornado,
  wahwah: Footprints,
  distort: Shell,
  speed: Rabbit,
  // Phase 3: the sort glyph, wide to narrow, as pixels settle by brightness.
  pixelsort: ArrowDownWideNarrow,
  pull: HandGrab,
  // Phase 5c adjustments: more literal, as photo tools are: a histogram for
  // levels, a spline for curves, a palette for hue and saturation, and sun
  // and moon for tinting highlights and shadows.
  levels: ChartColumn,
  curves: Spline,
  hsl: Palette,
  balance: SunMoon,
  // The filters (D-71), provisional until Phil picks: soft waves for
  // low-pass, a spiky trace for high-pass, one waveform picked out for
  // band-pass, a funnel for the notch, and an EQ's sliders.
  lowpass: Waves,
  highpass: Activity,
  bandpass: AudioWaveform,
  notch: Funnel,
  basstreble: SlidersVertical,
};

/** The effect's icon; effects added later fall back to sparkles. */
export const iconFor = (effect: string): IconComponent => ICONS[effect] ?? Sparkles;

/**
 * The paths of Lucide icons that the app also shows struck through
 * (Slashed.svelte), copied from Lucide 24 x 24 (ISC license). Lucide has no
 * apple-off or venetian-mask-off.
 */
export const OFF_PATHS = {
  /** The mask overlay (rubylith), hidden. */
  apple: ['M12 6.528V3a1 1 0 0 1 1-1h0', 'M18.237 21A15 15 0 0 0 22 11a6 6 0 0 0-10-4.472A6 6 0 0 0 2 11a15.1 15.1 0 0 0 3.763 10 3 3 0 0 0 3.648.648 5.5 5.5 0 0 1 5.178 0A3 3 0 0 0 18.237 21'],
  /** A mask, or one of its components, switched off. */
  venetianMask: [
    'M18 11c-1.5 0-2.5.5-3 2',
    'M4 6a2 2 0 0 0-2 2v4a5 5 0 0 0 5 5 8 8 0 0 1 5 2 8 8 0 0 1 5-2 5 5 0 0 0 5-5V8a2 2 0 0 0-2-2h-3a8 8 0 0 0-5 2 8 8 0 0 0-5-2z',
    'M6 11c1.5 0 2.5.5 3 2',
  ],
} as const;
