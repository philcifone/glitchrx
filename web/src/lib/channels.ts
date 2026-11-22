// Channel choices, shared by the recipe settings and the per-step override.
import type { Channels } from './types';

/** `[count, label, tooltip]` for each channel layout. */
export const CHANNEL_CHOICES: [Channels, string, string][] = [
  [1, 'Mono', 'Mono: every sample in one stream'],
  [2, 'Stereo', 'Stereo: alternate samples are left and right, processed separately'],
  [3, 'RGB', 'Three channels: with u8, red, green and blue are processed separately, so colours hold'],
];

/** Short name of a channel layout, for labels. */
export function channelName(n: Channels): string {
  return CHANNEL_CHOICES.find(([c]) => c === n)?.[1] ?? 'Mono';
}
