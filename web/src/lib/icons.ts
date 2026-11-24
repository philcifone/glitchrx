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
};

/** The effect's icon; effects added later fall back to sparkles. */
export const iconFor = (effect: string): IconComponent => ICONS[effect] ?? Sparkles;
