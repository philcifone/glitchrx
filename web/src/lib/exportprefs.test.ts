import { describe, expect, it } from 'vitest';
import { DEFAULT_PREFS, cleanPrefs, isLossy, optionsOf } from './exportprefs';

describe('export settings', () => {
  it('cleans what storage held', () => {
    expect(cleanPrefs(null)).toEqual(DEFAULT_PREFS);
    expect(cleanPrefs({ format: 'gif', avifSpeed: 40, webpQuality: 33.4, webpLossless: 'yes' })).toEqual({
      ...DEFAULT_PREFS,
      avifSpeed: 10,
      webpQuality: 33,
    });
  });

  it('sends only what applies to the format', () => {
    const p = { ...DEFAULT_PREFS, jpegQuality: 80, webpQuality: 70, avifQuality: 50, avifSpeed: 9 };
    expect(optionsOf({ ...p, format: 'tiff' })).toEqual({ format: 'tiff' });
    expect(optionsOf({ ...p, format: 'jpeg' })).toEqual({ format: 'jpeg', quality: 80 });
    expect(optionsOf({ ...p, format: 'webp' })).toEqual({ format: 'webp', quality: 70 });
    expect(optionsOf({ ...p, format: 'webp', webpLossless: true })).toEqual({ format: 'webp', lossless: true });
    expect(optionsOf({ ...p, format: 'avif' })).toEqual({ format: 'avif', quality: 50, speed: 9 });
  });

  it('knows which settings lose detail', () => {
    expect(isLossy({ ...DEFAULT_PREFS, format: 'png' })).toBe(false);
    expect(isLossy({ ...DEFAULT_PREFS, format: 'webp', webpLossless: true })).toBe(false);
    expect(isLossy({ ...DEFAULT_PREFS, format: 'webp' })).toBe(true);
    expect(isLossy({ ...DEFAULT_PREFS, format: 'avif' })).toBe(true);
  });
});
