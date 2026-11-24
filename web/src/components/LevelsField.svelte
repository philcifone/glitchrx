<script lang="ts">
  // The levels editor, laid out as in Photoshop: the histogram of the
  // picture reaching this step, with the input handles right under it
  // (black, midtone gamma, white), then the output handles (out_black,
  // out_white). Exact numbers sit under the handles.
  //
  // Drag anywhere on a strip to move the nearest handle there; double tap
  // to put it back on its default, like the sliders (D-55). Handles also
  // take the arrow keys. The midtone handle keeps its share of the stretch
  // when black or white move, so gamma stays what it was.
  import { app } from '../lib/app.svelte';
  import type { Histogram } from '../lib/api';
  import { countsFor, gammaToShare, histogramPath, shareToGamma } from '../lib/histogram';
  import type { ParamInfo } from '../lib/types';

  let {
    uid,
    specs,
    params,
  }: {
    uid: string;
    /** black, white, gamma, out_black, out_white (as the registry has them). */
    specs: ParamInfo[];
    params: Record<string, string>;
  } = $props();

  type Name = 'black' | 'white' | 'gamma' | 'out_black' | 'out_white';
  const spec = (n: Name) => specs.find((s) => s.name === n);
  const num = (n: Name) => Number(params[n] ?? spec(n)?.default ?? 0);

  const black = $derived(num('black'));
  const white = $derived(num('white'));
  const gamma = $derived(num('gamma'));
  const outBlack = $derived(num('out_black'));
  const outWhite = $derived(num('out_white'));
  /** The midtone handle, in input levels. */
  const mid = $derived(black + (white - black) * gammaToShare(gamma));

  // Histogram, refreshed a moment after each new preview (as in curves).
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
  const channel = $derived(params.channel ?? 'rgb');
  const PLOT_H = 100;
  const histPath = $derived(hist ? histogramPath(countsFor(hist, channel), PLOT_H) : '');

  /** Live while dragging; `commit` records one history entry. */
  function put(n: Name, v: number, commit: boolean) {
    app.setParam(uid, n, String(v), commit);
  }

  /** Where a value may go, given the other handles. */
  function clampFor(n: Name, v: number): number {
    // Two levels apart at least, so the midtone handle has room.
    if (n === 'black') return Math.max(0, Math.min(white - 2, Math.round(v)));
    if (n === 'white') return Math.min(255, Math.max(black + 2, Math.round(v)));
    if (n === 'gamma') return Math.min(10, Math.max(0.1, Math.round(v * 100) / 100));
    return Math.min(255, Math.max(0, Math.round(v)));
  }

  /** A strip position (0..255) as the value of handle `n`. */
  function valueAt(n: Name, level: number): number {
    if (n === 'gamma') return shareToGamma((level - black) / Math.max(1, white - black));
    return clampFor(n, level);
  }

  // ---- Dragging on a strip --------------------------------------------

  /** Handles are inset by this much, so the ends are not cut off. */
  const INSET = 9;

  let drag: { n: Name; moved: boolean } | null = null;
  let lastTap = { t: 0, x: 0 };

  function levelAt(e: PointerEvent, el: HTMLElement): number {
    const r = el.getBoundingClientRect();
    const f = (e.clientX - r.left - INSET) / Math.max(1, r.width - 2 * INSET);
    return Math.min(255, Math.max(0, f * 255));
  }

  function nearest(level: number, handles: [Name, number][]): Name {
    let best = handles[0];
    for (const h of handles) if (Math.abs(h[1] - level) < Math.abs(best[1] - level)) best = h;
    return best[0];
  }

  function down(e: PointerEvent, handles: [Name, number][]) {
    const el = e.currentTarget as HTMLElement;
    e.preventDefault();
    const level = levelAt(e, el);
    const n = nearest(level, handles);
    // A double tap puts the nearest handle back on its default.
    if (e.timeStamp - lastTap.t <= 300 && Math.abs(e.clientX - lastTap.x) <= 24) {
      lastTap = { t: 0, x: 0 };
      app.setParam(uid, n, null, true);
      return;
    }
    lastTap = { t: e.timeStamp, x: e.clientX };
    drag = { n, moved: false };
    el.setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const v = valueAt(drag.n, levelAt(e, e.currentTarget as HTMLElement));
    if (v !== num(drag.n)) {
      drag.moved = true;
      put(drag.n, v, false);
    }
  }

  function up() {
    if (drag?.moved) app.commit();
    drag = null;
  }

  /** Arrow keys on a focused handle: one level (or 0.05 gamma) a press. */
  function key(e: KeyboardEvent, n: Name) {
    const step = n === 'gamma' ? 0.05 : 1;
    // Gamma's handle moves left as gamma grows, so the keys follow the handle.
    const dir = n === 'gamma' ? -1 : 1;
    let d = 0;
    if (e.key === 'ArrowRight' || e.key === 'ArrowUp') d = step * dir;
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowDown') d = -step * dir;
    else return;
    e.preventDefault();
    put(n, clampFor(n, num(n) + d), true);
  }

  /** A typed number, checked like a handle; a bad one shows the old value. */
  function typed(el: HTMLInputElement, n: Name) {
    const v = Number(el.value);
    if (el.value.trim() !== '' && Number.isFinite(v)) put(n, clampFor(n, v), true);
    el.value = String(num(n));
  }

  const left = (level: number) => `calc(${INSET}px + (100% - ${2 * INSET}px) * ${level / 255})`;
</script>

{#snippet handle(n: Name, level: number, shade: string, label: string, text: string)}
  <span
    class="handle {shade}"
    style:left={left(level)}
    role="slider"
    tabindex="0"
    aria-label={label}
    aria-valuenow={num(n)}
    aria-valuetext={text}
    onkeydown={(e) => key(e, n)}
  >
    <svg viewBox="0 0 14 11" aria-hidden="true"><path d="M7 1 L13 10 L1 10 Z" /></svg>
  </span>
{/snippet}

<div class="levels">
  <div class="plot">
    <svg viewBox="0 0 255 {PLOT_H}" preserveAspectRatio="none" aria-hidden="true">
      <rect class="bg" x="0" y="0" width="255" height={PLOT_H} />
      {#if histPath}<path class="hist {channel}" d={histPath} />{/if}
      <!-- Shade what clips to black and to white. -->
      {#if black > 0}<rect class="clip" x="0" y="0" width={black} height={PLOT_H} />{/if}
      {#if white < 255}<rect class="clip" x={white} y="0" width={255 - white} height={PLOT_H} />{/if}
      <line class="midline" x1={mid} y1="0" x2={mid} y2={PLOT_H} />
    </svg>
  </div>

  <div
    class="strip"
    role="group"
    aria-label="Input levels"
    onpointerdown={(e) =>
      down(e, [
        ['black', black],
        ['gamma', mid],
        ['white', white],
      ])}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  >
    <span class="ramp"></span>
    {@render handle('black', black, 'dark', 'Input black', String(black))}
    {@render handle('gamma', mid, 'grey', 'Midtones (gamma)', String(gamma))}
    {@render handle('white', white, 'light', 'Input white', String(white))}
  </div>
  <div class="nums three">
    <input class="num" type="number" min="0" max="254" value={black} aria-label="Input black" onchange={(e) => typed(e.currentTarget, 'black')} />
    <input class="num" type="number" min="0.1" max="10" step="0.01" value={gamma} aria-label="Gamma" onchange={(e) => typed(e.currentTarget, 'gamma')} />
    <input class="num" type="number" min="1" max="255" value={white} aria-label="Input white" onchange={(e) => typed(e.currentTarget, 'white')} />
  </div>

  <div class="out">
    <span class="caption dim">Output</span>
    <input class="num" type="number" min="0" max="255" value={outBlack} aria-label="Output black" onchange={(e) => typed(e.currentTarget, 'out_black')} />
    <div
      class="strip"
      role="group"
      aria-label="Output levels"
      onpointerdown={(e) =>
        down(e, [
          ['out_black', outBlack],
          ['out_white', outWhite],
        ])}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
    >
      <span class="ramp"></span>
      {@render handle('out_black', outBlack, 'dark', 'Output black', String(outBlack))}
      {@render handle('out_white', outWhite, 'light', 'Output white', String(outWhite))}
    </div>
    <input class="num" type="number" min="0" max="255" value={outWhite} aria-label="Output white" onchange={(e) => typed(e.currentTarget, 'out_white')} />
  </div>
</div>

<style>
  .levels {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  /* Same inset as the strips, so a level sits at the same x in both. */
  .plot {
    padding: 0 9px;
  }

  .plot svg {
    display: block;
    width: 100%;
    height: 84px;
  }

  .bg {
    fill: var(--fill);
  }

  .hist {
    fill: var(--fg-dim);
    opacity: 0.45;
  }

  .hist.red {
    fill: #cc241d;
  }

  .hist.green {
    fill: #98971a;
  }

  .hist.blue {
    fill: #458588;
  }

  .clip {
    fill: rgb(0 0 0 / 0.28);
  }

  .midline {
    stroke: var(--fg-dim);
    stroke-width: 0.75;
    stroke-dasharray: 3 3;
    vector-effect: non-scaling-stroke;
    opacity: 0.6;
  }

  /* A strip: the grey ramp, with the handles hanging under it. The whole
     strip takes the touch, so a finger need not hit the small triangle. */
  .strip {
    position: relative;
    height: 26px;
    touch-action: none;
    cursor: ew-resize;
  }

  .ramp {
    position: absolute;
    left: 9px;
    right: 9px;
    top: 2px;
    height: 7px;
    border-radius: 2px;
    background: linear-gradient(to right, #000, #fff);
    box-shadow: inset 0 0 0 0.5px var(--line);
  }

  .handle {
    position: absolute;
    top: 10px;
    width: 16px;
    height: 13px;
    margin-left: -8px;
    display: block;
    outline-offset: 2px;
  }

  .handle svg {
    display: block;
    width: 100%;
    height: 100%;
  }

  .handle path {
    stroke: var(--fg-dim);
    stroke-width: 1.2;
    stroke-linejoin: round;
  }

  .dark path {
    fill: #1d2021;
  }

  .grey path {
    fill: #928374;
  }

  .light path {
    fill: #fbf7ec;
  }

  .nums {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }

  .num {
    width: 7ch;
    flex: none;
    text-align: right;
  }

  .three .num:nth-child(2) {
    text-align: center;
  }

  .three .num:first-child {
    text-align: left;
  }

  .out {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
  }

  .caption {
    flex-basis: 100%;
    font-size: 0.75rem;
  }

  .out .strip {
    flex: 1;
    min-width: 0;
    margin-top: 6px;
  }

  .out .num:first-of-type {
    text-align: left;
  }
</style>
