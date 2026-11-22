<script lang="ts">
  // The preview stage: shows the latest render (or the original while
  // comparing) at the current zoom, takes the gestures on the photo, draws
  // the region band while one is dragged, and hosts the mask painter while
  // a step's mask is being edited. The preview settles to an exact render
  // by itself, so there is no button for that.
  //
  // Gestures (SPEC 9.6), the same with a finger or the mouse:
  // - press and hold: the original, until released (as in Instagram)
  // - double tap, the magnifier or Z: fit to 100% and back
  // - two fingers: pinch to zoom, and pan; ctrl + wheel or a trackpad pinch
  //   zooms too
  // - zoomed in, one finger (or the middle mouse button) pans, so does the
  //   wheel
  // While masking, one finger works the mask tool instead, and the zoom
  // stays put between strokes; two fingers still pinch and pan.
  //
  // Zoomed in, the picture underneath is the whole-photo preview enlarged
  // (soft), with a sharp patch on top: the visible part cut from the
  // full-resolution render, asked for once the view holds still.
  import { onMount } from 'svelte';
  import ZoomIn from '@lucide/svelte/icons/zoom-in';
  import ZoomOut from '@lucide/svelte/icons/zoom-out';
  import { app } from '../lib/app.svelte';
  import { pan, percent, pinch, placement, spread, wheelZoom, type Point, type Spread, type View } from '../lib/view';
  import MaskPainter from './MaskPainter.svelte';

  let { onopen }: { onopen: () => void } = $props();
  let box: HTMLDivElement;
  let painter = $state<ReturnType<typeof MaskPainter>>();
  /** The cursor the painter asks for while masking. */
  let paintCursor = $state('default');

  onMount(() => {
    // Tell the app how big the preview area is, so renders fit it.
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      app.setViewport(Math.round(width), Math.round(height), window.devicePixelRatio || 1);
    });
    ro.observe(box);
    // Listeners that must be able to cancel: Svelte adds wheel listeners as
    // passive. Safari's own pinch events would zoom the whole page.
    const noPageZoom = (e: Event) => e.preventDefault();
    box.addEventListener('wheel', wheel, { passive: false });
    box.addEventListener('gesturestart', noPageZoom);
    box.addEventListener('gesturechange', noPageZoom);
    return () => {
      ro.disconnect();
      box.removeEventListener('wheel', wheel);
      box.removeEventListener('gesturestart', noPageZoom);
      box.removeEventListener('gesturechange', noPageZoom);
    };
  });

  const g = $derived(app.geometry);
  /** Where the picture sits on the stage, in CSS pixels. */
  const rect = $derived(g ? placement(app.view, g) : { x: 0, y: 0, w: 0, h: 0 });
  const shown = $derived(app.comparing && app.originalUrl ? app.originalUrl : app.imageUrl);
  /** The sharp patch to lay on top, if it is current. */
  const patch = $derived(
    !app.zoomed
      ? null
      : app.comparing
        ? app.originalCrop
        : app.crop && app.crop.rev >= app.imageRevision
          ? app.crop
          : null,
  );
  /** Past 100%, photo pixels show as squares rather than blurred. */
  const pixelated = $derived(!!g && app.view.z > g.z100 * 1.01);

  // -------------------------------------------------------------------------
  // Gestures. The stage captures each pointer, so a drag that leaves it
  // still ends here.

  const HOLD_MS = 250; // press this long for the original
  const SLOP = 8; // pixels a tap may wander
  const DOUBLE_TAP_MS = 300;

  type Gesture =
    | { kind: 'press'; start: Point; timer: number } // a tap, a hold, or the start of a pan
    | { kind: 'pan'; last: Point }
    | { kind: 'hold' } // showing the original
    | { kind: 'paint' } // the mask painter has it
    | { kind: 'pinch'; start: View; from: Spread }
    | { kind: 'spent' }; // fingers still down after a pinch: ignored until all lift
  let gesture: Gesture | null = null;
  /** Fingers (or the mouse) down on the stage, in stage pixels. */
  const points = new Map<number, Point>();
  let lastTap: { t: number; p: Point } | null = null;
  let panning = $state(false);

  function onStage(e: PointerEvent | WheelEvent): Point {
    const b = box.getBoundingClientRect();
    return { x: e.clientX - b.left, y: e.clientY - b.top };
  }

  function twoFingers(): Spread {
    const [a, b] = [...points.values()];
    return spread(a, b);
  }

  /** Whatever one finger was doing stops: a stroke is dropped, the original hides. */
  function stopOne() {
    if (gesture?.kind === 'press') clearTimeout(gesture.timer);
    if (gesture?.kind === 'hold') app.compare(false);
    if (gesture?.kind === 'paint') painter?.cancel();
  }

  function down(e: PointerEvent) {
    if (!g || !shown || (e.target as HTMLElement).closest('button')) return;
    if (e.pointerType === 'mouse' && e.button !== 0 && e.button !== 1) return;
    box.setPointerCapture(e.pointerId);
    const p = onStage(e);
    points.set(e.pointerId, p);
    if (points.size === 2) {
      stopOne();
      gesture = { kind: 'pinch', start: app.view, from: twoFingers() };
      return;
    }
    if (points.size > 2) return;
    if (e.button === 1) {
      e.preventDefault(); // no autoscroll
      gesture = { kind: 'pan', last: p };
      panning = true;
    } else if (app.masking) {
      gesture = painter ? { kind: 'paint' } : { kind: 'spent' };
      painter?.down(e);
    } else {
      const timer = window.setTimeout(() => {
        if (gesture?.kind !== 'press') return;
        gesture = { kind: 'hold' };
        app.compare(true);
      }, HOLD_MS);
      gesture = { kind: 'press', start: p, timer };
    }
  }

  function move(e: PointerEvent) {
    const p = onStage(e);
    if (!points.has(e.pointerId)) {
      // The mouse moving with no button down: the brush outline, the cursor.
      if (app.masking) painter?.move(e);
      return;
    }
    points.set(e.pointerId, p);
    if (!gesture || !g) return;
    switch (gesture.kind) {
      case 'pinch':
        if (points.size >= 2) app.setView(pinch(gesture.start, gesture.from, twoFingers(), g));
        break;
      case 'paint':
        painter?.move(e);
        break;
      case 'press':
        if (Math.hypot(p.x - gesture.start.x, p.y - gesture.start.y) <= SLOP) break;
        clearTimeout(gesture.timer);
        // A drag, not a tap or a hold: it pans (by the whole move so far),
        // unless at fit, where there is nothing to pan.
        if (app.zoomed) {
          panning = true;
          gesture = panBy({ kind: 'pan', last: gesture.start }, p);
        } else {
          gesture = { kind: 'spent' };
        }
        break;
      case 'pan':
        gesture = panBy(gesture, p);
        break;
    }
  }

  function panBy(pan_: { kind: 'pan'; last: Point }, p: Point): Gesture {
    if (g) app.setView(pan(app.view, p.x - pan_.last.x, p.y - pan_.last.y, g));
    return { kind: 'pan', last: p };
  }

  function up(e: PointerEvent) {
    if (!points.has(e.pointerId)) return;
    points.delete(e.pointerId);
    const was = gesture;
    if (points.size === 0) {
      gesture = null;
      panning = false;
    } else if (was?.kind === 'pinch') {
      gesture = { kind: 'spent' }; // one finger left: wait for it to lift
    }
    const cancelled = e.type === 'pointercancel';
    if (was?.kind === 'paint') {
      if (cancelled) painter?.cancel();
      else painter?.up(e);
    } else if (was?.kind === 'hold') {
      app.compare(false);
    } else if (was?.kind === 'press') {
      clearTimeout(was.timer);
      if (!cancelled) tap(onStage(e));
    }
  }

  /** Two taps close together toggle the zoom about where they landed. */
  function tap(p: Point) {
    const now = performance.now();
    const near = lastTap && now - lastTap.t < DOUBLE_TAP_MS && Math.hypot(p.x - lastTap.p.x, p.y - lastTap.p.y) < 30;
    lastTap = near ? null : { t: now, p };
    if (near) app.toggleZoom(p);
  }

  function wheel(e: WheelEvent) {
    if (!g || !shown) return;
    e.preventDefault();
    // Lines and pages to pixels, roughly, for mice that report them.
    const unit = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? box.clientHeight : 1;
    if (e.ctrlKey || e.metaKey) {
      // A trackpad pinch arrives as ctrl + wheel.
      app.setView(wheelZoom(app.view, e.deltaY * unit, onStage(e), g));
    } else if (app.zoomed) {
      app.setView(pan(app.view, -e.deltaX * unit, -e.deltaY * unit, g));
    }
  }

  const cursor = $derived(
    app.masking ? paintCursor : panning ? 'grabbing' : app.zoomed ? 'grab' : 'default',
  );
</script>

<div
  class="box"
  bind:this={box}
  style:cursor
  onpointerdown={down}
  onpointermove={move}
  onpointerup={up}
  onpointercancel={up}
  onpointerleave={() => painter?.leave()}
  oncontextmenu={(e) => e.preventDefault()}
  role="presentation"
>
  {#if shown}
    <img
      class="photo"
      src={shown}
      alt={app.comparing ? 'Original photo' : 'Glitched preview'}
      draggable="false"
      style:left="{rect.x}px"
      style:top="{rect.y}px"
      style:width="{rect.w}px"
      style:height="{rect.h}px"
    />
    {#if patch}
      <!-- Part of the full-resolution render, placed over the same part of
           the picture; it moves with a pan until the next one arrives. -->
      <img
        class="photo patch"
        class:pixelated
        src={patch.url}
        alt=""
        draggable="false"
        style:left="{rect.x + patch.rect.x * rect.w}px"
        style:top="{rect.y + patch.rect.y * rect.h}px"
        style:width="{patch.rect.w * rect.w}px"
        style:height="{patch.rect.h * rect.h}px"
      />
    {/if}
    {#if app.regionHint}
      <!-- The region is a share of the raster's bytes, and rows follow each
           other, so it is the same share of the height: a horizontal band. -->
      {@const r = app.regionHint}
      <div
        class="band"
        aria-hidden="true"
        style:left="{rect.x}px"
        style:top="{rect.y}px"
        style:width="{rect.w}px"
        style:height="{rect.h}px"
      >
        <div class="shade" style:top="0" style:height="{r.s}%"></div>
        <div class="edge" style:top="{r.s}%" style:height="{r.e - r.s}%"></div>
        <div class="shade" style:top="{r.e}%" style:bottom="0"></div>
      </div>
    {/if}
    {#if app.masking && !app.comparing && app.maskTarget && g}
      <!-- What is being painted, Done and compare are in the top bar:
           nothing floats over the photo while painting. -->
      <MaskPainter bind:this={painter} bind:cursor={paintCursor} {rect} stage={g.box} />
    {/if}
  {:else if !app.doc}
    <div class="empty">
      <p>Open a photo to start: JPEG, PNG or TIFF.</p>
      <button class="primary" onclick={onopen}>Open a photo</button>
      <p class="dim hint">
        {app.desktop ? 'Or drop one here.' : 'Or drop one here. Photos go to this glitchrx server only.'}
      </p>
    </div>
  {/if}

  {#if app.busy || app.working}
    <div class="progress" aria-hidden="true"></div>
  {/if}
  {#if app.working && !app.masking}
    <div class="working" role="status">{app.working}</div>
  {:else if app.heavy}
    <div class="working heavy" role="status">
      {app.heavy}.
      <button onclick={() => app.render('exact')}>Try exact</button>
    </div>
  {/if}

  {#if app.doc && app.imageUrl && g}
    <button
      class="zoom"
      class:on={app.zoomed}
      onclick={() => app.toggleZoom()}
      title={app.zoomed
        ? 'Back to fit (Z, or double tap)'
        : 'Zoom to 100% (Z, or double tap; pinch to zoom). Press and hold the photo for the original.'}
      aria-label={app.zoomed ? 'Zoom to fit' : 'Zoom to 100%'}
    >
      {#if app.zoomed}
        <ZoomOut size={18} strokeWidth={1.75} />
        <span>{percent(app.view, g)}%</span>
      {:else}
        <ZoomIn size={18} strokeWidth={1.75} />
      {/if}
    </button>
  {/if}
</div>

<style>
  .box {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    /* The stage handles pinch and pan itself. */
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }

  /* Placed by the view; may be far larger than the stage when zoomed. */
  .photo {
    position: absolute;
    max-width: none;
    display: block;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }

  .patch.pixelated {
    image-rendering: pixelated;
  }

  .empty {
    text-align: center;
    padding: var(--pad);
    color: var(--fg);
  }

  .hint {
    font-size: 0.9em;
    max-width: 32ch;
    margin-inline: auto;
  }

  /* An indeterminate bar along the top edge while rendering. */
  .progress {
    position: absolute;
    top: 0;
    left: 0;
    height: 2px;
    width: 30%;
    background: var(--accent);
    animation: slide 0.9s linear infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .progress {
      animation: none;
      width: 100%;
      opacity: 0.6;
    }
  }

  /* Floating "glass" elements over the photo: blurred, rounded pills. */
  .working,
  .zoom {
    background: var(--glass);
    backdrop-filter: saturate(180%) blur(16px);
    -webkit-backdrop-filter: saturate(180%) blur(16px);
    box-shadow: var(--shadow);
    border-radius: 999px;
  }

  .working {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    padding: 6px 14px;
    font-size: 0.9rem;
    white-space: nowrap;
  }

  /* Region band: shade outside it, thin accent edges along it. */
  .band {
    position: absolute;
    pointer-events: none;
  }

  .shade {
    position: absolute;
    left: 0;
    right: 0;
    background: rgb(0 0 0 / 0.5);
  }

  .edge {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 2px solid var(--accent);
    border-bottom: 2px solid var(--accent);
  }

  .heavy {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 4px 4px 14px;
    max-width: calc(100% - 24px);
    white-space: normal;
    color: var(--warn);
  }

  .heavy button {
    border-radius: 999px;
    min-height: 30px;
    flex: none;
  }

  /* The magnifier: a round glass button, a pill with the zoom when zoomed. */
  .zoom {
    position: absolute;
    right: 12px;
    bottom: 12px;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 40px;
    min-height: 40px;
    padding: 6px 11px;
    font-size: 0.9rem;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }

  .zoom span {
    color: var(--fg);
  }

  .zoom:hover:not(:disabled) {
    background: var(--glass);
  }
</style>
