<script lang="ts">
  // Crop (SPEC 5d), as in Photos or Lightroom: the whole photo, unbent,
  // with a frame over it. Drag a corner to size it, the middle to move it,
  // and, free of a ratio, an edge to move that side. Ratio, Reset, Cancel
  // and Crop sit under the photo; Enter and Escape work too.
  //
  // The frame is worked out in the photo's own pixels, so a ratio holds
  // exactly whatever the screen size, and stored as shares of the photo.
  //
  // Photo | Result (D-69): crop the photo before the steps, or frame the
  // finished picture. For the photo, the picture shown is unbent, because
  // the steps then run on the crop: outside it there is nothing bent to
  // show. For the result, it is the whole glitch, which the frame cuts
  // from without changing it.
  import FlipHorizontal from '@lucide/svelte/icons/rectangle-horizontal';
  import FlipVertical from '@lucide/svelte/icons/rectangle-vertical';
  import { app } from '../lib/app.svelte';
  import { CROP_RATIOS, cropRect, ratioLabel, ratioValue } from '../lib/canvas';

  const doc = $derived(app.doc!);
  const W = $derived(doc.width);
  const H = $derived(doc.height);

  // The area the photo is fitted into, measured.
  let aw = $state(0);
  let ah = $state(0);
  const PAD = 22;
  const scale = $derived(Math.max(1e-6, Math.min((aw - 2 * PAD) / W, (ah - 2 * PAD) / H)));
  const img = $derived({ x: (aw - W * scale) / 2, y: (ah - H * scale) / 2, w: W * scale, h: H * scale });

  /** The frame in photo pixels. */
  const r = $derived.by(() => {
    const c = app.cropping!.rect;
    return { x: c.x * W, y: c.y * H, w: c.w * W, h: c.h * H };
  });
  const px = $derived(cropRect(app.cropping!.rect, W, H));

  // Ratio: width / height in pixels, or null when free. `choice` is the
  // menu entry; `portrait` turns the others (except Original, which is
  // the photo's own shape).
  let choice = $state('free');
  let portrait = $state(false);
  const sides = $derived(CROP_RATIOS.find((c) => c.id === choice)?.sides ?? null);
  const ratio = $derived(ratioValue(sides, portrait, W / H));
  const turnable = $derived(!!sides && sides[0] !== 0 && sides[0] !== sides[1]);
  // Start the flip on the photo's own way round.
  $effect.pre(() => {
    if (doc) portrait = H > W;
  });

  type Px = { x: number; y: number; w: number; h: number };

  function store(p: Px) {
    app.setCropRect({ x: p.x / W, y: p.y / H, w: p.w / W, h: p.h / H });
  }

  /** The largest frame of `rr` inside the photo, centred where the frame is. */
  function fitRatio(rr: number | null) {
    if (rr === null) return;
    let w = Math.min(W, H * rr);
    let h = w / rr;
    if (h > H) {
      h = H;
      w = h * rr;
    }
    const cx = r.x + r.w / 2;
    const cy = r.y + r.h / 2;
    store({ x: clamp(cx - w / 2, 0, W - w), y: clamp(cy - h / 2, 0, H - h), w, h });
  }

  function pick(c: string) {
    choice = c;
    queueMicrotask(() => fitRatio(ratio));
  }

  function flip() {
    portrait = !portrait;
    queueMicrotask(() => fitRatio(ratio));
  }

  function reset() {
    choice = 'free';
    store({ x: 0, y: 0, w: W, h: H });
  }

  const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

  // ---------------------------------------------------------------------
  // Dragging

  type Grab = 'move' | 'nw' | 'ne' | 'sw' | 'se' | 'n' | 's' | 'e' | 'w';
  let grab: { what: Grab; x: number; y: number; start: Px; id: number } | null = null;
  let area: HTMLDivElement;

  function down(e: PointerEvent, what: Grab) {
    e.preventDefault();
    e.stopPropagation();
    grab = { what, x: e.clientX, y: e.clientY, start: { ...r }, id: e.pointerId };
    area.setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    e.stopPropagation();
    if (!grab || e.pointerId !== grab.id) return;
    const dx = (e.clientX - grab.x) / scale;
    const dy = (e.clientY - grab.y) / scale;
    const s = grab.start;
    // Smallest frame: about 40 screen pixels, and never under 1 photo pixel.
    const min = Math.max(1, 40 / scale);
    const w = grab.what;
    if (w === 'move') {
      store({ ...s, x: clamp(s.x + dx, 0, W - s.w), y: clamp(s.y + dy, 0, H - s.h) });
    } else if (w.length === 2) {
      // A corner: the opposite corner stays put.
      const sx = w[1] === 'e' ? 1 : -1;
      const sy = w[0] === 's' ? 1 : -1;
      const ax = sx > 0 ? s.x : s.x + s.w;
      const ay = sy > 0 ? s.y : s.y + s.h;
      const maxW = sx > 0 ? W - ax : ax;
      const maxH = sy > 0 ? H - ay : ay;
      let nw = ((sx > 0 ? s.x + s.w + dx : s.x + dx) - ax) * sx;
      let nh = ((sy > 0 ? s.y + s.h + dy : s.y + dy) - ay) * sy;
      if (ratio !== null) {
        // Follow whichever way the finger went further.
        nw = Math.max(nw, nh * ratio);
        nw = clamp(nw, min * Math.max(1, ratio), Math.min(maxW, maxH * ratio));
        nh = nw / ratio;
      } else {
        nw = clamp(nw, min, maxW);
        nh = clamp(nh, min, maxH);
      }
      store({ x: sx > 0 ? ax : ax - nw, y: sy > 0 ? ay : ay - nh, w: nw, h: nh });
    } else if (ratio === null) {
      // An edge, free of a ratio.
      const p = { ...s };
      if (w === 'e') p.w = clamp(s.w + dx, min, W - s.x);
      if (w === 's') p.h = clamp(s.h + dy, min, H - s.y);
      if (w === 'w') {
        const x = clamp(s.x + dx, 0, s.x + s.w - min);
        p.w = s.x + s.w - x;
        p.x = x;
      }
      if (w === 'n') {
        const y = clamp(s.y + dy, 0, s.y + s.h - min);
        p.h = s.y + s.h - y;
        p.y = y;
      }
      store(p);
    }
  }

  function up(e: PointerEvent) {
    e.stopPropagation();
    if (grab && e.pointerId === grab.id) grab = null;
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      app.applyCrop();
    } else if (e.key === 'Escape') {
      e.preventDefault();
      app.cancelCrop();
    }
  }

  const frame = $derived({ x: img.x + r.x * scale, y: img.y + r.y * scale, w: r.w * scale, h: r.h * scale });
  const CORNERS: Grab[] = ['nw', 'ne', 'sw', 'se'];
  const EDGES: Grab[] = ['n', 's', 'e', 'w'];
</script>

<svelte:window onkeydown={key} />

<!-- The stage's own gestures (zoom, hold for the original) stay out. -->
<div class="crop" onpointerdown={(e) => e.stopPropagation()} role="presentation">
  <div
    class="area"
    bind:this={area}
    bind:clientWidth={aw}
    bind:clientHeight={ah}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    role="presentation"
  >
    {#if app.cropUrl}
      <img
        src={app.cropUrl}
        alt={app.cropping?.result ? 'The whole result' : 'Uncropped original'}
        draggable="false"
        style:left="{img.x}px"
        style:top="{img.y}px"
        style:width="{img.w}px"
        style:height="{img.h}px"
      />
    {/if}
    <div
      class="frame"
      style:left="{frame.x}px"
      style:top="{frame.y}px"
      style:width="{frame.w}px"
      style:height="{frame.h}px"
      onpointerdown={(e) => down(e, 'move')}
      role="button"
      tabindex="-1"
      aria-label="Crop frame: drag to move"
    >
      <span class="third v1"></span><span class="third v2"></span>
      <span class="third h1"></span><span class="third h2"></span>
      {#if ratio === null}
        {#each EDGES as g (g)}
          <span class="edge {g}" onpointerdown={(e) => down(e, g)} role="presentation"><i></i></span>
        {/each}
      {/if}
      {#each CORNERS as g (g)}
        <span class="corner {g}" onpointerdown={(e) => down(e, g)} role="presentation"><i></i></span>
      {/each}
    </div>
  </div>

  <div class="bar">
    <div class="row">
      <span class="lbl">Crop the</span>
      <div class="segmented stage" role="radiogroup" aria-label="Crop the photo or the result">
        <button
          role="radio"
          aria-checked={!app.cropping?.result}
          class:on={!app.cropping?.result}
          onclick={() => app.setCropResult(false)}
          title="Crop the photo, then glitch what is left: the glitch is made for the crop"
        >
          Photo
        </button>
        <button
          role="radio"
          aria-checked={!!app.cropping?.result}
          class:on={!!app.cropping?.result}
          onclick={() => app.setCropResult(true)}
          title="Glitch the whole photo, then crop: the glitch stays as it is"
        >
          Result
        </button>
      </div>
    </div>
    <div class="row">
      <div class="segmented" role="radiogroup" aria-label="Crop ratio">
        {#each CROP_RATIOS as c (c.id)}
          <button role="radio" aria-checked={choice === c.id} class:on={choice === c.id} onclick={() => pick(c.id)}>
            {ratioLabel(c.sides, portrait)}
          </button>
        {/each}
      </div>
      <button
        class="icon"
        onclick={flip}
        disabled={!turnable}
        aria-label={portrait ? 'Make it landscape' : 'Make it portrait'}
        title={portrait ? 'Landscape' : 'Portrait'}
      >
        {#if portrait}<FlipVertical size={20} strokeWidth={1.75} />{:else}<FlipHorizontal size={20} strokeWidth={1.75} />{/if}
      </button>
    </div>
    <div class="row">
      <button class="plain" onclick={() => app.cancelCrop()}>Cancel</button>
      <button class="plain" onclick={reset}>Reset</button>
      <span class="size dim">{px.w}x{px.h}</span>
      <button class="primary" onclick={() => app.applyCrop()}>Crop</button>
    </div>
  </div>
</div>

<style>
  .crop {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    z-index: 3;
    background: var(--stage, var(--bg));
  }

  .area {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
  }

  img {
    position: absolute;
    pointer-events: none;
  }

  /* The shade outside the frame is the frame's own shadow, clipped by the area. */
  .frame {
    position: absolute;
    box-shadow: 0 0 0 100vmax rgb(0 0 0 / 0.55);
    outline: 1px solid rgb(255 255 255 / 0.9);
    cursor: move;
  }

  .third {
    position: absolute;
    background: rgb(255 255 255 / 0.45);
    pointer-events: none;
  }

  .v1,
  .v2 {
    top: 0;
    bottom: 0;
    width: 1px;
  }

  .v1 {
    left: 33.333%;
  }

  .v2 {
    left: 66.667%;
  }

  .h1,
  .h2 {
    left: 0;
    right: 0;
    height: 1px;
  }

  .h1 {
    top: 33.333%;
  }

  .h2 {
    top: 66.667%;
  }

  /* Corners: a bold L inside a fingertip-sized target. */
  .corner {
    position: absolute;
    width: 44px;
    height: 44px;
    touch-action: none;
  }

  .corner i {
    position: absolute;
    width: 20px;
    height: 20px;
    border: 0 solid #fff;
  }

  .nw {
    left: -22px;
    top: -22px;
    cursor: nwse-resize;
  }

  .nw i {
    left: 19px;
    top: 19px;
    border-left-width: 3px;
    border-top-width: 3px;
  }

  .ne {
    right: -22px;
    top: -22px;
    cursor: nesw-resize;
  }

  .ne i {
    right: 19px;
    top: 19px;
    border-right-width: 3px;
    border-top-width: 3px;
  }

  .sw {
    left: -22px;
    bottom: -22px;
    cursor: nesw-resize;
  }

  .sw i {
    left: 19px;
    bottom: 19px;
    border-left-width: 3px;
    border-bottom-width: 3px;
  }

  .se {
    right: -22px;
    bottom: -22px;
    cursor: nwse-resize;
  }

  .se i {
    right: 19px;
    bottom: 19px;
    border-right-width: 3px;
    border-bottom-width: 3px;
  }

  /* Edges: a short bar in the middle of each side, with a wide target. */
  .edge {
    position: absolute;
    display: flex;
    align-items: center;
    justify-content: center;
    touch-action: none;
  }

  .edge i {
    background: #fff;
    border-radius: 2px;
  }

  .edge.n,
  .edge.s {
    left: 50%;
    width: 64px;
    height: 32px;
    margin-left: -32px;
    cursor: ns-resize;
  }

  .edge.n {
    top: -16px;
  }

  .edge.s {
    bottom: -16px;
  }

  .edge.n i,
  .edge.s i {
    width: 24px;
    height: 3px;
  }

  .edge.e,
  .edge.w {
    top: 50%;
    width: 32px;
    height: 64px;
    margin-top: -32px;
    cursor: ew-resize;
  }

  .edge.e {
    right: -16px;
  }

  .edge.w {
    left: -16px;
  }

  .edge.e i,
  .edge.w i {
    width: 3px;
    height: 24px;
  }

  .bar {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 12px 10px;
    background: var(--glass);
    backdrop-filter: saturate(180%) blur(16px);
    -webkit-backdrop-filter: saturate(180%) blur(16px);
    border-top: var(--hairline);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .row .segmented {
    flex: 1;
    min-width: 0;
  }

  .row .segmented button {
    padding-left: 4px;
    padding-right: 4px;
    font-size: 0.85rem;
  }

  .lbl {
    flex: none;
    font-size: 0.85rem;
    color: var(--fg-dim);
  }

  .size {
    flex: 1;
    text-align: center;
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }
</style>
