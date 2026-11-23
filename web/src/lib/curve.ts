// Tone curves for the `curves` adjustment: the text form the core reads
// (`IN:OUT/IN:OUT/...`, 0 to 255, inputs rising) and the same monotone
// cubic the core draws through the points (Fritsch-Carlson), so the editor
// shows the curve the export will use. Drawing only: the server's table is
// what counts, so this needs no bit-exactness.

export type CurvePoint = [number, number];

/** The most points the core accepts. */
export const MAX_POINTS = 16;

/** Read `0:0/128:150/255:255`; null if it is not a valid curve. */
export function parseCurve(text: string): CurvePoint[] | null {
  const pts: CurvePoint[] = [];
  for (const part of text.split('/')) {
    const m = /^\s*(\d{1,3})\s*:\s*(\d{1,3})\s*$/.exec(part);
    if (!m) return null;
    const p: CurvePoint = [Number(m[1]), Number(m[2])];
    if (p[0] > 255 || p[1] > 255) return null;
    if (pts.length && p[0] <= pts[pts.length - 1][0]) return null;
    pts.push(p);
  }
  return pts.length >= 2 && pts.length <= MAX_POINTS ? pts : null;
}

export function formatCurve(pts: CurvePoint[]): string {
  return pts.map(([x, y]) => `${Math.round(x)}:${Math.round(y)}`).join('/');
}

/** Fritsch-Carlson tangents, as in the core's curves.rs. */
function tangents(p: CurvePoint[]): number[] {
  const n = p.length;
  const d = p.slice(1).map((q, k) => (q[1] - p[k][1]) / (q[0] - p[k][0]));
  const m = new Array<number>(n).fill(0);
  m[0] = d[0];
  m[n - 1] = d[n - 2];
  for (let k = 1; k < n - 1; k++) m[k] = d[k - 1] * d[k] > 0 ? (d[k - 1] + d[k]) / 2 : 0;
  for (let k = 0; k < n - 1; k++) {
    if (d[k] === 0) {
      m[k] = m[k + 1] = 0;
      continue;
    }
    const a = m[k] / d[k];
    const b = m[k + 1] / d[k];
    const s = a * a + b * b;
    if (s > 9) {
      const t = 3 / Math.sqrt(s);
      m[k] = t * a * d[k];
      m[k + 1] = t * b * d[k];
    }
  }
  return m;
}

/** A function giving the curve's output (0..255) at an input (0..255). */
export function curveFn(p: CurvePoint[]): (x: number) => number {
  const m = tangents(p);
  const n = p.length;
  return (x) => {
    if (x <= p[0][0]) return p[0][1];
    if (x >= p[n - 1][0]) return p[n - 1][1];
    let k = 0;
    while (x >= p[k + 1][0]) k++;
    const [x0, y0] = p[k];
    const [x1, y1] = p[k + 1];
    const h = x1 - x0;
    const t = (x - x0) / h;
    const t2 = t * t;
    const t3 = t2 * t;
    return (
      (2 * t3 - 3 * t2 + 1) * y0 + (t3 - 2 * t2 + t) * h * m[k] + (-2 * t3 + 3 * t2) * y1 + (t3 - t2) * h * m[k + 1]
    );
  };
}
