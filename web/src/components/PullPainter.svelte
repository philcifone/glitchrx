<script lang="ts">
  // Drawing pull strokes on the preview (SPEC 8.2, D-64). Covers the stage
  // like the mask painter, and like it takes only the one-finger gestures
  // from the preview, which keeps two fingers for pinch and pan.
  //
  // A stroke goes into the recipe while it is drawn (uncommitted, at most
  // once a frame), so the preview pulls along behind the finger; releasing
  // makes it one undo step. Nothing is drawn over the photo but the brush
  // ring: the picture itself shows what the stroke does.
  //
  // The brush is sized against the screen, as the mask brush is: zoomed in,
  // the same setting pulls a smaller area of the photo.
  import { app } from '../lib/app.svelte';
  import { addPoint, roundCoord } from '../lib/mask';
  import type { PullStroke } from '../lib/types';

  let {
    rect,
    cursor: cursorStyle = $bindable('none'),
  }: {
    /** Where the whole photo sits on the stage: points are shares of it (D-52). */
    rect: { x: number; y: number; w: number; h: number };
    /** The mouse cursor the stage should show. */
    cursor?: string;
  } = $props();

  let root = $state<HTMLDivElement>();
  /** Where the ring is drawn (mouse hover, or the finger while drawing). */
  let ring = $state<{ x: number; y: number } | null>(null);
  let live: PullStroke | null = null;
  let frame = 0;

  const long = $derived(Math.max(rect.w, rect.h));
  /** The brush's size on the photo: its setting, shrunk by the zoom. */
  const size = $derived(app.pullTool.size / app.view.z);

  function onStage(e: PointerEvent): { x: number; y: number } {
    const b = root!.getBoundingClientRect();
    return { x: e.clientX - b.left, y: e.clientY - b.top };
  }

  /** Send the stroke so far, once per frame however fast the events come. */
  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (live) app.livePull({ ...live, points: [...live.points] });
    });
  }

  export function down(e: PointerEvent) {
    if (e.button !== 0 || !root) return;
    const p = onStage(e);
    ring = p;
    live = {
      size: Math.max(0.0001, Math.round(size * 10000) / 10000),
      strength: app.pullTool.strength,
      points: [roundCoord((p.x - rect.x) / rect.w), roundCoord((p.y - rect.y) / rect.h)],
    };
    app.livePull(live);
  }

  export function move(e: PointerEvent) {
    if (!root) return;
    const p = onStage(e);
    if (e.pointerType === 'mouse' || live) ring = p;
    if (!live) return;
    // Coalesced events carry every sample since the last frame.
    const events = e.getCoalescedEvents?.() ?? [e];
    // A point every tenth of the brush (or 1.5 px): the core steps along
    // the path at a quarter of the radius anyway.
    const minPx = Math.max(1.5, live.size * long * 0.1);
    for (const ev of events.length ? events : [e]) {
      const q = onStage(ev);
      addPoint(live.points, (q.x - rect.x) / rect.w, (q.y - rect.y) / rect.h, minPx, rect.w, rect.h);
    }
    schedule();
  }

  export function up(e: PointerEvent) {
    const stroke = live;
    live = null;
    cancelAnimationFrame(frame);
    frame = 0;
    if (e.pointerType !== 'mouse') ring = null;
    app.endPull(stroke);
  }

  /** A second finger came down, or the system took the gesture: no stroke. */
  export function cancel() {
    live = null;
    cancelAnimationFrame(frame);
    frame = 0;
    ring = null;
    app.endPull(null);
  }

  export function leave() {
    if (!live) ring = null;
  }

  $effect(() => {
    cursorStyle = 'none';
  });
</script>

<div class="painter" bind:this={root} aria-label="Pull: drag on the photo">
  {#if ring}
    <div
      class="ring"
      style:left="{ring.x}px"
      style:top="{ring.y}px"
      style:width="{size * long}px"
      style:height="{size * long}px"
    ></div>
  {/if}
</div>

<style>
  /* Covers the stage; the stage takes the gestures and passes them on. */
  .painter {
    position: absolute;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .ring {
    position: absolute;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    border: 1.5px solid white;
    box-shadow:
      0 0 0 1px rgb(0 0 0 / 0.5),
      inset 0 0 0 1px rgb(0 0 0 / 0.5);
    pointer-events: none;
  }
</style>
