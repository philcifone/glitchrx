// The canvas size, worked out as the server does (Raster::fit in the
// core): integer arithmetic, so the GUI and the server agree to the pixel
// on how large the work-size copy is.

/** Fit `w` x `h` inside `maxW` x `maxH`, keeping the aspect and never enlarging. */
export function fitSize(w: number, h: number, maxW: number, maxH: number): [number, number] {
  w = Math.max(1, w);
  h = Math.max(1, h);
  if (w <= maxW && h <= maxH) return [w, h];
  if (w * maxH >= h * maxW) return [Math.max(1, maxW), Math.max(1, Math.floor((h * maxW) / w))];
  return [Math.max(1, Math.floor((w * maxH) / h)), Math.max(1, maxH)];
}

/** The work sizes offered under the photo (the longest side; null is full size). */
export const WORK_SIZES: { size: number | null; label: string }[] = [
  { size: null, label: 'Full' },
  { size: 4096, label: '4K' },
  { size: 2048, label: '2K' },
  { size: 1024, label: '1K' },
];
