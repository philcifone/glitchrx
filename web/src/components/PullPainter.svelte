<script lang="ts">
  // Drawing a pull on the preview (SPEC 8.2, D-64, D-67). Covers the stage
  // like the mask painter, and like it takes only the one-finger gestures
  // from the preview, which keeps two fingers for pinch and pan.
  //
  // Two tools:
  // - Draw: a stroke goes into the recipe while it is drawn (uncommitted,
  //   at most once a frame), so the preview pulls along behind the finger;
  //   releasing makes it one undo step. Only the brush ring is drawn: the
  //   picture itself shows what the stroke does.
  // - Pins: a grid over the photo. Touching near a grid point picks it up
  //   as a new pin; touching near a pin (where it was put, or its home)
  //   picks that pin up again. Dragging moves it, live; dragging it back
  //   home removes it. Each move is one undo step.
  //
  // The brush is sized against the screen, as the mask brush is: zoomed in,
  // the same setting pulls a smaller area of the photo.
  import { app } from '../lib/app.svelte';
  import { addPoint, roundCoord } from '../lib/mask';
  import { gridAxes, pinAt, snap } from '../lib/pull';
  import type { PullPin, PullStroke } from '../lib/types';

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
  const pinning = $derived(app.pullTool.mode === 'pins');

  // ---------------------------------------------------------------------
  // Pins

  /** How near (in screen pixels) a finger must come to a pin to pick it up. */
  const REACH_TOUCH = 26;
  const REACH_MOUSE = 14;
  /** A pin dropped this near its home (screen pixels) is removed. */
  const HOME = 10;

  const axes = $derived(gridAxes(rect.w, rect.h, app.pullTool.grid));
  const pins = $derived<PullPin[]>(app.pullTarget?.step.pin ?? []);
  /** The pin being moved: its index, the pins around it, and where the finger holds it. */
  let held = $state<{ i: number; base: PullPin[]; grip: { x: number; y: number } } | null>(null);

  /** A share of the photo, on the stage. */
  const at = (q: [number, number]) => ({ x: rect.x + q[0] * rect.w, y: rect.y + q[1] * rect.h });
  /** A point on the stage, as a share of the photo. */
  const share = (p: { x: number; y: number }): [number, number] => [roundCoord((p.x - rect.x) / rect.w), roundCoord((p.y - rect.y) / rect.h)];

  function pinDown(e: PointerEvent, p: { x: number; y: number }) {
    const current = $state.snapshot(pins) as PullPin[];
    const i = pinAt(current, p, at, e.pointerType === 'mouse' ? REACH_MOUSE : REACH_TOUCH);
    if (i >= 0) {
      const to = at(current[i].to);
      held = { i, base: current, grip: { x: p.x - to.x, y: p.y - to.y } };
    } else {
      const home = snap((p.x - rect.x) / rect.w, (p.y - rect.y) / rect.h, axes);
      const pin: PullPin = { size: Math.max(0.0001, Math.round(size * 10000) / 10000), from: home, to: home };
      const h = at(home);
      held = { i: current.length, base: [...current, pin], grip: { x: p.x - h.x, y: p.y - h.y } };
    }
  }

  /** The pins with the held one where the finger has it now. */
  function moved(p: { x: number; y: number }): PullPin[] {
    if (!held) return [];
    const to = share({ x: p.x - held.grip.x, y: p.y - held.grip.y });
    return held.base.map((pin, k) => (k === held!.i ? { ...pin, to } : pin));
  }

  let latest: PullPin[] | null = null;

  function pinMove(p: { x: number; y: number }) {
    latest = moved(p);
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (latest) app.livePins(latest);
    });
  }

  function pinUp(p: { x: number; y: number }) {
    if (!held) return;
    let out = moved(p);
    const pin = out[held.i];
    const a = at(pin.from);
    const b = at(pin.to);
    // Back home (or never moved): the pin goes.
    if (Math.hypot(a.x - b.x, a.y - b.y) <= HOME) out = out.filter((_, k) => k !== held!.i);
    held = null;
    latest = null;
    app.endPins(out);
  }

  // ---------------------------------------------------------------------
  // Strokes

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
    if (pinning) {
      pinDown(e, p);
      return;
    }
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
    if (pinning) {
      if (held) pinMove(p);
      return;
    }
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
    cancelAnimationFrame(frame);
    frame = 0;
    if (pinning) {
      if (root) pinUp(onStage(e));
      return;
    }
    const stroke = live;
    live = null;
    if (e.pointerType !== 'mouse') ring = null;
    app.endPull(stroke);
  }

  /** A second finger came down, or the system took the gesture: nothing changes. */
  export function cancel() {
    cancelAnimationFrame(frame);
    frame = 0;
    if (held) {
      held = null;
      latest = null;
      app.endPins(null);
    }
    live = null;
    ring = null;
    app.endPull(null);
  }

  export function leave() {
    if (!live) ring = null;
  }

  $effect(() => {
    cursorStyle = pinning ? (held ? 'grabbing' : 'crosshair') : 'none';
  });

  // Leaving Pins mode mid-drag: keep nothing half done.
  $effect(() => {
    if (!pinning && held) cancel();
  });
</script>

<div class="painter" bind:this={root} aria-label={pinning ? 'Pull: drag a grid point or a pin' : 'Pull: drag on the photo'}>
  {#if pinning}
    <svg class="pins" width="100%" height="100%">
      {#if app.pullTool.show === 'lines'}
        {#each axes.xs as x (x)}
          <line class="shade" x1={rect.x + x * rect.w} y1={rect.y} x2={rect.x + x * rect.w} y2={rect.y + rect.h} />
          <line class="grid" x1={rect.x + x * rect.w} y1={rect.y} x2={rect.x + x * rect.w} y2={rect.y + rect.h} />
        {/each}
        {#each axes.ys as y (y)}
          <line class="shade" x1={rect.x} y1={rect.y + y * rect.h} x2={rect.x + rect.w} y2={rect.y + y * rect.h} />
          <line class="grid" x1={rect.x} y1={rect.y + y * rect.h} x2={rect.x + rect.w} y2={rect.y + y * rect.h} />
        {/each}
      {:else if app.pullTool.show === 'dots'}
        {#each axes.ys as y (y)}
          {#each axes.xs as x (x)}
            <circle class="dot" cx={rect.x + x * rect.w} cy={rect.y + y * rect.h} r="2.2" />
          {/each}
        {/each}
      {/if}
      {#each pins as pin, i (i)}
        {@const a = at(pin.from)}
        {@const b = at(pin.to)}
        {@const active = held?.i === i}
        {#if active}
          <!-- How far this pin reaches. -->
          <circle class="reach" cx={b.x} cy={b.y} r={(pin.size / 2) * long} />
        {/if}
        <line class="shade" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
        <line class="tie" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
        <circle class="home" cx={a.x} cy={a.y} r="4" />
        <circle class="handle" class:active cx={b.x} cy={b.y} r={active ? 9 : 7} />
      {/each}
    </svg>
  {:else if ring}
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

  .pins {
    position: absolute;
    inset: 0;
    overflow: visible;
  }

  /* Light marks on a dark edge, so they read on any picture. */
  .shade {
    stroke: rgb(0 0 0 / 0.45);
    stroke-width: 2.5;
  }

  .grid {
    stroke: rgb(255 255 255 / 0.6);
    stroke-width: 1;
  }

  .dot {
    fill: rgb(255 255 255 / 0.85);
    stroke: rgb(0 0 0 / 0.5);
    stroke-width: 1;
  }

  .tie {
    stroke: white;
    stroke-width: 1.5;
  }

  .home {
    fill: none;
    stroke: white;
    stroke-width: 1.5;
  }

  .handle {
    fill: var(--accent);
    stroke: white;
    stroke-width: 2;
  }

  .reach {
    fill: none;
    stroke: white;
    stroke-width: 1;
    stroke-dasharray: 4 4;
    opacity: 0.8;
  }
</style>
