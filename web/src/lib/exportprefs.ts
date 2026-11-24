// The Export sheet's format and encoder settings, remembered per browser
// (a convenience, like the brush size), and turned into what the server
// takes: only the settings that apply to the chosen format.

import type { ExportFormat, ExportOptions } from './types';

/** What Export saves: the finished picture, or a project (.glrx, D-63). */
export type ExportKind = 'picture' | 'project';

export interface ExportPrefs {
  kind: ExportKind;
  format: ExportFormat;
  jpegQuality: number;
  webpQuality: number;
  webpLossless: boolean;
  avifQuality: number;
  avifSpeed: number;
}

/** The server's own defaults (core EncodeSettings). */
export const DEFAULT_PREFS: ExportPrefs = {
  kind: 'picture',
  format: 'tiff',
  jpegQuality: 95,
  webpQuality: 90,
  webpLossless: false,
  avifQuality: 80,
  avifSpeed: 6,
};

const FORMATS: ExportFormat[] = ['tiff', 'png', 'jpeg', 'webp', 'avif'];

const whole = (v: unknown, lo: number, hi: number, fallback: number) =>
  typeof v === 'number' && Number.isFinite(v) ? Math.min(hi, Math.max(lo, Math.round(v))) : fallback;

/** Settings from storage, with anything stale or broken replaced by defaults. */
export function cleanPrefs(saved: unknown): ExportPrefs {
  const s = (saved && typeof saved === 'object' ? saved : {}) as Partial<Record<keyof ExportPrefs, unknown>>;
  const d = DEFAULT_PREFS;
  return {
    kind: s.kind === 'project' ? 'project' : d.kind,
    format: FORMATS.includes(s.format as ExportFormat) ? (s.format as ExportFormat) : d.format,
    jpegQuality: whole(s.jpegQuality, 1, 100, d.jpegQuality),
    webpQuality: whole(s.webpQuality, 1, 100, d.webpQuality),
    webpLossless: typeof s.webpLossless === 'boolean' ? s.webpLossless : d.webpLossless,
    avifQuality: whole(s.avifQuality, 1, 100, d.avifQuality),
    avifSpeed: whole(s.avifSpeed, 1, 10, d.avifSpeed),
  };
}

/** What to send for an export. */
export function optionsOf(p: ExportPrefs): ExportOptions {
  // A project has no encoder settings; the format only keeps the request whole.
  if (p.kind === 'project') return { format: p.format, project: true };
  switch (p.format) {
    case 'jpeg':
      return { format: 'jpeg', quality: p.jpegQuality };
    case 'webp':
      return p.webpLossless ? { format: 'webp', lossless: true } : { format: 'webp', quality: p.webpQuality };
    case 'avif':
      return { format: 'avif', quality: p.avifQuality, speed: p.avifSpeed };
    default:
      return { format: p.format };
  }
}

/** Whether the export throws detail away (and so smooths fine glitch texture). */
export const isLossy = (p: ExportPrefs) =>
  p.kind === 'picture' &&
  (p.format === 'jpeg' || p.format === 'avif' || (p.format === 'webp' && !p.webpLossless));

/** The WebP format's limit on each side. */
export const WEBP_MAX_SIDE = 16383;
