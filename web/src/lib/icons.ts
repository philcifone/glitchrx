// An icon for each effect, for the effect palette and step headers.
//
// Picked by Phil from a contact sheet of Lucide icons: playful rather than
// literal (a mountain for echo, a drum for stutter). The dice stays
// reserved for re-rolling seeds, and the wand for shaking settings.

import Mountain from '@lucide/svelte/icons/mountain';
import Copy from '@lucide/svelte/icons/copy';
import IterationCw from '@lucide/svelte/icons/iteration-cw';
import Mic from '@lucide/svelte/icons/mic';
import Eclipse from '@lucide/svelte/icons/eclipse';
import Blocks from '@lucide/svelte/icons/blocks';
import Rewind from '@lucide/svelte/icons/rewind';
import MoveHorizontal from '@lucide/svelte/icons/move-horizontal';
import Orbit from '@lucide/svelte/icons/orbit';
import Drum from '@lucide/svelte/icons/drum';
import Sparkles from '@lucide/svelte/icons/sparkles';

export type IconComponent = typeof Mountain;

const ICONS: Record<string, IconComponent> = {
  echo: Mountain,
  delay: Copy,
  reverb: IterationCw,
  amplify: Mic,
  invert: Eclipse,
  bitcrush: Blocks,
  backwards: Rewind,
  shift: MoveHorizontal,
  shuffle: Orbit,
  stutter: Drum,
};

/** The effect's icon; effects added later fall back to sparkles. */
export const iconFor = (effect: string): IconComponent => ICONS[effect] ?? Sparkles;
