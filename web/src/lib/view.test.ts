import { describe, expect, it } from 'vitest';
import {
  FIT,
  clampView,
  geometry,
  isZoomed,
  pan,
  percent,
  pinch,
  placement,
  spread,
  toPhoto,
  toggle,
  visible,
  wheelZoom,
  zoomAt,
} from './view';

// A phone-ish stage (400 x 600 CSS px, 3x screen) and a 6000 x 4000 photo:
// at fit the photo is 400 x 266.7, centred vertically.
const g = geometry({ w: 400, h: 600 }, { w: 6000, h: 4000 }, 3);
const close = (a: number, b: number) => expect(a).toBeCloseTo(b, 6);

describe('geometry', () => {
  it('fits the photo and knows 100%', () => {
    close(g.fit.w, 400);
    close(g.fit.h, 4000 * (400 / 6000));
    // 6000 photo pixels across 400 CSS px = 1200 device px: 5x to reach 1:1.
    close(g.z100, 5);
    close(g.zMax, 40);
  });

  it('copes with a stage not laid out yet', () => {
    const empty = geometry({ w: 0, h: 0 }, { w: 6000, h: 4000 }, 2);
    expect(empty.fit).toEqual({ w: 0, h: 0 });
  });

  it('enlarges a photo smaller than the stage', () => {
    close(geometry({ w: 960, h: 640 }, { w: 480, h: 320 }, 1).fit.w, 960);
  });
});

describe('placement and clamping', () => {
  it('centres the picture at fit', () => {
    const r = placement(FIT, g);
    close(r.x, 0);
    close(r.y, (600 - g.fit.h) / 2);
  });

  it('never leaves a gap the picture could cover', () => {
    // Zoomed 2x: 800 px wide, so the centre may go from 0.25 to 0.75. The
    // height (533 px) is still under the stage's 600: it stays centred.
    const v = clampView({ z: 2, x: 0.01, y: 0.9 }, g);
    close(v.x, 0.25);
    close(v.y, 0.5);
    expect(clampView({ z: 100, x: 0.5, y: 0.5 }, g).z).toBe(g.zMax);
    expect(clampView({ z: 0.2, x: 0.5, y: 0.5 }, g).z).toBe(1);
  });
});

describe('gestures', () => {
  it('zooms about the point under the finger', () => {
    const p = { x: 300, y: 250 };
    const before = toPhoto(p, FIT, g);
    const v = zoomAt(FIT, 3, p, g);
    const after = toPhoto(p, v, g);
    close(after.x, before.x);
    close(after.y, before.y);
  });

  it('pinches: zoom by spread, the picture stays under the fingers', () => {
    const start = zoomAt(FIT, 2, { x: 200, y: 300 }, g);
    const from = spread({ x: 150, y: 300 }, { x: 250, y: 300 });
    const under = toPhoto(from.mid, start, g);
    // Fingers twice as far apart, and both moved 20 px left.
    const now = spread({ x: 80, y: 300 }, { x: 280, y: 300 });
    const v = pinch(start, from, now, g);
    close(v.z, 4);
    const still = toPhoto(now.mid, v, g);
    close(still.x, under.x);
    close(still.y, under.y);
  });

  it('pans with the finger and stops at the edges', () => {
    const v = zoomAt(FIT, 4, { x: 200, y: 300 }, g);
    const moved = pan(v, -40, 0, g);
    // 40 px left on a 1600 px wide picture is 1/40 of it.
    close(moved.x - v.x, 40 / 1600);
    expect(pan(v, 10000, 0, g).x).toBeCloseTo(200 / 1600, 6);
  });

  it('wheel zoom is gentle and bounded per event', () => {
    const p = { x: 200, y: 300 };
    close(wheelZoom(FIT, -100, p, g).z, Math.SQRT2);
    close(wheelZoom(FIT, -1000, p, g).z, Math.SQRT2);
    expect(wheelZoom(FIT, 100, p, g).z).toBe(1);
  });

  it('the magnifier goes to 100% and back', () => {
    const v = toggle(FIT, { x: 200, y: 300 }, g);
    close(v.z, 5);
    expect(percent(v, g)).toBe(100);
    expect(isZoomed(v)).toBe(true);
    expect(toggle(v, { x: 0, y: 0 }, g)).toEqual(FIT);
    // A small photo: 100% is barely a zoom, so it goes to 2x.
    const small = geometry({ w: 400, h: 600 }, { w: 600, h: 400 }, 2);
    close(toggle(FIT, { x: 200, y: 300 }, small).z, 2);
  });
});

describe('visible', () => {
  it('is the whole picture at fit', () => {
    const { crop, screen } = visible(FIT, g);
    expect(crop).toEqual({ x: 0, y: 0, w: 1, h: 1 });
    close(screen.w, 400);
    close(screen.h, g.fit.h);
  });

  it('is the part on screen when zoomed', () => {
    // 4x about the centre: a quarter of the width; the height (1067 px)
    // overflows the 600 px stage too.
    const { crop, screen } = visible({ z: 4, x: 0.5, y: 0.5 }, g);
    close(crop.w, 0.25);
    close(crop.x, 0.375);
    close(crop.h, 600 / (g.fit.h * 4));
    close(screen.w, 400);
    close(screen.h, 600);
  });
});
