// Zoom and pan (SPEC 9.6). Pure geometry, kept apart from the components so
// it can be tested without a browser: where the picture sits on the stage
// for a view, and how gestures change the view.
//
// A view is a zoom factor and the picture point at the centre of the stage.
// Zoom 1 fits the whole picture in the stage; 2 shows it twice as large.
// Stage points are CSS pixels from the stage's top-left corner. Picture
// points are fractions of its width and height, as mask positions are.

export interface View {
  z: number;
  x: number;
  y: number;
}
export interface Point {
  x: number;
  y: number;
}
export interface Size {
  w: number;
  h: number;
}
export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The whole picture, centred. */
export const FIT: View = { z: 1, x: 0.5, y: 0.5 };

/** What the view math needs to know about the stage and the photo. */
export interface Geometry {
  /** The stage, in CSS pixels. */
  box: Size;
  /** The picture at zoom 1, in CSS pixels. */
  fit: Size;
  /** The zoom at which one photo pixel covers one device pixel (100%). */
  z100: number;
  /** The most zoom allowed: 800%, and never less than 4x the fit. */
  zMax: number;
}

const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

export function geometry(box: Size, photo: Size, dpr: number): Geometry {
  if (box.w <= 0 || box.h <= 0 || photo.w <= 0 || photo.h <= 0) {
    return { box, fit: { w: 0, h: 0 }, z100: 1, zMax: 4 };
  }
  // Fit may enlarge a picture smaller than the stage, as photo apps do.
  const s = Math.min(box.w / photo.w, box.h / photo.h);
  const fit = { w: photo.w * s, h: photo.h * s };
  const z100 = photo.w / (fit.w * dpr);
  return { box, fit, z100, zMax: Math.max(4, z100 * 8) };
}

export const isZoomed = (v: View) => v.z > 1.001;

/** Where the picture is drawn on the stage. Zoomed in, it reaches past the edges. */
export function placement(v: View, g: Geometry): Rect {
  const w = g.fit.w * v.z;
  const h = g.fit.h * v.z;
  return { x: g.box.w / 2 - v.x * w, y: g.box.h / 2 - v.y * h, w, h };
}

/**
 * How far past the stage's edge a zoomed picture's edge may be dragged:
 * this share of the stage's side, but no more than `OVERPAN_MAX` CSS
 * pixels. Stopping exactly at the edge made the photo's border hard to
 * work on, a mask along the bottom above all (Phil, 2026-11-25).
 */
export const OVERPAN = 0.25;
export const OVERPAN_MAX = 160;

/**
 * The view made valid: zoom within range, and the picture's edges no
 * further inside the stage than the overpan. At the fit the picture is
 * centred; the overpan grows in from there (full by 1.5x), so pinching
 * out to the fit settles smoothly rather than jumping.
 */
export function clampView(v: View, g: Geometry): View {
  const z = clamp(v.z, 1, g.zMax);
  const ramp = clamp((z - 1) / 0.5, 0, 1);
  const centre = (c: number, len: number, box: number) => {
    if (len <= 0) return 0.5;
    const slack = ramp * Math.min(box * OVERPAN, OVERPAN_MAX);
    // The picture's edge may sit `slack` pixels inside the stage's edge.
    // A side smaller than the stage moves `slack` either way of centre.
    const half = len > box ? (box / 2 - slack) / len : 0.5 - slack / len;
    return clamp(c, half, 1 - half);
  };
  return { z, x: centre(v.x, g.fit.w * z, g.box.w), y: centre(v.y, g.fit.h * z, g.box.h) };
}

/** The picture point under stage point `p`. */
export function toPhoto(p: Point, v: View, g: Geometry): Point {
  const r = placement(v, g);
  return { x: (p.x - r.x) / r.w, y: (p.y - r.y) / r.h };
}

/** The view at zoom `z` that puts picture point `u` under stage point `p`, as near as the edges allow. */
function anchor(u: Point, p: Point, z: number, g: Geometry): View {
  const zz = clamp(z, 1, g.zMax);
  return clampView(
    {
      z: zz,
      x: u.x - (p.x - g.box.w / 2) / (g.fit.w * zz),
      y: u.y - (p.y - g.box.h / 2) / (g.fit.h * zz),
    },
    g,
  );
}

/** Zoom to `z`, keeping the picture point under `p` where it is (wheel, double tap). */
export function zoomAt(v: View, z: number, p: Point, g: Geometry): View {
  return anchor(toPhoto(p, v, g), p, z, g);
}

/** Two fingers: the point midway between them, and how far apart they are. */
export interface Spread {
  mid: Point;
  apart: number;
}

export function spread(a: Point, b: Point): Spread {
  return { mid: { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 }, apart: Math.hypot(a.x - b.x, a.y - b.y) };
}

/**
 * Pinch from `start`, the view when the second finger landed. The zoom
 * follows how far apart the fingers are, and the picture point first
 * between them stays between them, so pinching also pans.
 */
export function pinch(start: View, from: Spread, now: Spread, g: Geometry): View {
  const z = start.z * (now.apart / Math.max(1, from.apart));
  return anchor(toPhoto(from.mid, start, g), now.mid, z, g);
}

/** Drag the picture by `dx`, `dy` stage pixels. */
export function pan(v: View, dx: number, dy: number, g: Geometry): View {
  return clampView({ z: v.z, x: v.x - dx / (g.fit.w * v.z), y: v.y - dy / (g.fit.h * v.z) }, g);
}

/** Wheel zoom (a trackpad pinch, or ctrl + wheel): about 1.4x per mouse notch. */
export function wheelZoom(v: View, deltaY: number, p: Point, g: Geometry): View {
  const step = clamp(deltaY, -50, 50);
  return zoomAt(v, v.z * 2 ** (-step / 100), p, g);
}

/** The magnifier and a double tap: fit to 100% (at least twice the fit), and back. */
export function toggle(v: View, p: Point, g: Geometry): View {
  return isZoomed(v) ? FIT : zoomAt(v, Math.max(2, g.z100), p, g);
}

/** The zoom as a share of the photo's own pixels: 100% shows one per device pixel. */
export function percent(v: View, g: Geometry): number {
  return Math.round((v.z / g.z100) * 100);
}

/**
 * The part of the picture on screen, as fractions of the photo (`crop`),
 * and how big it is on screen in CSS pixels (`screen`).
 */
export function visible(v: View, g: Geometry): { crop: Rect; screen: Size } {
  const r = placement(v, g);
  const x0 = clamp(-r.x / r.w, 0, 1);
  const x1 = clamp((g.box.w - r.x) / r.w, 0, 1);
  const y0 = clamp(-r.y / r.h, 0, 1);
  const y1 = clamp((g.box.h - r.y) / r.h, 0, 1);
  return {
    crop: { x: x0, y: y0, w: x1 - x0, h: y1 - y0 },
    screen: { w: (x1 - x0) * r.w, h: (y1 - y0) * r.h },
  };
}
