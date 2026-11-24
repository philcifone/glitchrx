<script lang="ts">
  // The curve editor for the `curves` adjustment, as in Photoshop or
  // Lightroom: input runs left to right, output bottom to top. Drag a
  // point to move it; touch the empty grid to add one there and drag it;
  // the selected point shows its numbers and can be removed. The ends can
  // move too: pulling the first point up lifts the blacks.
  //
  // The SVG takes the touch for itself (touch-action: none), so dragging a
  // point never scrolls the card.
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { MAX_POINTS, curveFn, formatCurve, parseCurve, type CurvePoint } from '../lib/curve';
  import { app } from '../lib/app.svelte';
  import type { Histogram } from '../lib/api';
  import { countsFor, histogramPath } from '../lib/histogram';

  let {
    uid,
    text,
    oninput,
    onchange,
  }: {
    /** The step, for its histogram and channel. */
    uid: string;
    text: string;
    /** Live change while dragging. */
    oninput: (v: string) => void;
    /** A finished change, recorded in history. */
    onchange: (v: string) => void;
  } = $props();

  const IDENTITY: CurvePoint[] = [
    [0, 0],
    [255, 255],
  ];
  const points = $derived(parseCurve(text) ?? IDENTITY);
  const GRID = [1, 2, 3, 4, 5, 6, 7].map((k) => ({ at: (k * 255) / 8, major: k % 2 === 0 }));
  let selected = $state(-1);
  const sel = $derived(selected >= 0 && selected < points.length ? points[selected] : null);

  /** The curve as an SVG path, sampled every two input steps. */
  const path = $derived.by(() => {
    const f = curveFn(points);
    let d = '';
    for (let x = 0; x <= 256; x += 2) {
      const xi = Math.min(x, 255);
      d += `${d ? 'L' : 'M'}${xi},${(255 - f(xi)).toFixed(2)}`;
    }
    return d;
  });

  // The histogram of the picture reaching this step, behind the curve, as
  // in Photoshop: refreshed a moment after each new preview, since the
  // steps before this one may have changed it.
  let hist = $state<Histogram | null>(null);
  let histTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void app.imageRevision;
    void app.doc?.id;
    clearTimeout(histTimer);
    histTimer = setTimeout(async () => {
      hist = await app.histogram(uid);
    }, 250);
    return () => clearTimeout(histTimer);
  });

  const channel = $derived(app.step(uid)?.params.channel ?? 'rgb');
  /** The histogram as a filled shape under a 255 x 255 box. */
  const histPath = $derived(hist ? histogramPath(countsFor(hist, channel), 255) : '');

  let svg: SVGSVGElement;
  let dragging = -1;
  let moved = false;

  // The view box has a margin, so points at the edges are not cut off.
  const PAD = 6;
  const SPAN = 255 + 2 * PAD;

  /**
   * A pointer position in curve units, 0..255 each way. The SVG's own
   * screen transform maps it, so letterboxing inside a box that is not
   * quite square cannot shift it.
   */
  function at(e: PointerEvent): CurvePoint {
    const m = svg.getScreenCTM();
    const q = m ? new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse()) : { x: 0, y: 255 };
    const x = q.x;
    const y = 255 - q.y;
    return [Math.min(255, Math.max(0, x)), Math.min(255, Math.max(0, y))];
  }

  /** The point within reach of `p` (a fingertip, about 7% of the box), if any. */
  function near(p: CurvePoint): number {
    const reach = 18;
    let best = -1;
    let dist = reach * reach;
    points.forEach(([x, y], i) => {
      const d = (x - p[0]) ** 2 + (y - p[1]) ** 2;
      if (d <= dist) {
        dist = d;
        best = i;
      }
    });
    return best;
  }

  function down(e: PointerEvent) {
    e.preventDefault();
    const p = at(e);
    let i = near(p);
    if (i < 0) {
      // Add a point where the finger is, between its neighbours.
      const x = Math.round(p[0]);
      if (points.length >= MAX_POINTS || points.some(([px]) => px === x)) return;
      const next = [...points, [x, Math.round(p[1])] as CurvePoint].sort((a, b) => a[0] - b[0]);
      i = next.findIndex(([px]) => px === x);
      oninput(formatCurve(next));
      moved = true;
    } else {
      moved = false;
    }
    selected = i;
    dragging = i;
    svg.setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (dragging < 0) return;
    const [x, y] = at(e);
    const pts = points.map((q) => [...q] as CurvePoint);
    // Inputs keep rising: a point stays strictly between its neighbours.
    const lo = dragging > 0 ? pts[dragging - 1][0] + 1 : 0;
    const hi = dragging < pts.length - 1 ? pts[dragging + 1][0] - 1 : 255;
    pts[dragging] = [Math.min(hi, Math.max(lo, Math.round(x))), Math.round(y)];
    const v = formatCurve(pts);
    if (v !== text) {
      moved = true;
      oninput(v);
    }
  }

  function up() {
    if (dragging >= 0 && moved) onchange(text);
    dragging = -1;
  }

  function removeSelected() {
    if (selected < 0 || points.length <= 2) return;
    onchange(formatCurve(points.filter((_, i) => i !== selected)));
    selected = -1;
  }
</script>

<div class="curve">
  <svg
    bind:this={svg}
    viewBox="{-PAD} {-PAD} {SPAN} {SPAN}"
    role="application"
    aria-label="Tone curve: drag a point, or touch the grid to add one"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  >
    <rect class="bg" x="0" y="0" width="255" height="255" rx="6" />
    {#if histPath}<path class="hist {channel}" d={histPath} />{/if}
    <!-- A grid to read the curve by, in eighths with stronger quarters.
         Only to look at: points do not snap to it. -->
    {#each GRID as g (g.at)}
      <line class="grid" class:major={g.major} x1={g.at} y1="0" x2={g.at} y2="255" />
      <line class="grid" class:major={g.major} x1="0" y1={g.at} x2="255" y2={g.at} />
    {/each}
    <line class="diag" x1="0" y1="255" x2="255" y2="0" />
    <path class="line" d={path} />
    {#each points as [x, y], i (i)}
      <circle class="pt" class:on={i === selected} cx={x} cy={255 - y} r={i === selected ? 7 : 5.5} />
    {/each}
  </svg>
  <div class="info">
    {#if sel}
      <span class="dim">In</span> <span class="v">{sel[0]}</span>
      <span class="dim">Out</span> <span class="v">{sel[1]}</span>
      <span class="grow"></span>
      <button class="icon" onclick={removeSelected} disabled={points.length <= 2} aria-label="Remove this point" title="Remove this point">
        <Trash2 size={17} strokeWidth={1.75} />
      </button>
    {:else}
      <span class="dim">Touch the grid to add a point; drag to move it.</span>
    {/if}
  </div>
</div>

<style>
  .curve {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  svg {
    width: 100%;
    max-width: 320px;
    aspect-ratio: 1;
    align-self: center;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    cursor: crosshair;
  }

  .bg {
    fill: var(--fill);
  }

  /* The dim text colour, not the hairline one: hairlines are tuned to
     barely show against a card, and the curve's fill is close to it. */
  .grid {
    stroke: var(--fg-dim);
    stroke-width: 0.6;
    opacity: 0.3;
  }

  .grid.major {
    stroke-width: 0.9;
    opacity: 0.5;
  }

  .hist {
    fill: var(--fg-dim);
    opacity: 0.22;
  }

  /* Muted Gruvbox for one channel. */
  .hist.red {
    fill: #cc241d;
  }

  .hist.green {
    fill: #98971a;
  }

  .hist.blue {
    fill: #458588;
  }

  .diag {
    stroke: var(--fg-dim);
    stroke-width: 0.75;
    stroke-dasharray: 3 4;
    opacity: 0.6;
  }

  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2.25;
    stroke-linejoin: round;
  }

  .pt {
    fill: var(--card);
    stroke: var(--accent);
    stroke-width: 2;
  }

  .pt.on {
    fill: var(--accent);
  }

  .info {
    display: flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    font-size: 0.85rem;
  }

  .v {
    font-variant-numeric: tabular-nums;
    min-width: 2.2em;
  }

  .grow {
    flex: 1;
  }
</style>
