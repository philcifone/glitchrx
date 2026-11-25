<script lang="ts">
  // Drawing a pull on the preview (SPEC 8.2, D-64, D-67, D-68). Covers the
  // stage like the mask painter, and like it takes only the one-finger
  // gestures from the preview, which keeps two fingers for pinch and pan.
  //
  // Strokes and pins are one list, in the order made: whatever is done last
  // goes over the rest. Two tools:
  // - Draw: a stroke goes into the recipe while it is drawn (uncommitted,
  //   at most once a frame), so the preview pulls along behind the finger;
  //   releasing makes it one undo step. It joins the selected brush, or
  //   starts a new one, which is then selected. Each stroke has a marker
  //   where the finger lifted; tapping one selects its brush (all its
  //   markers light up, with their reach), and tapping an empty spot
  //   selects none. With markers switched off nothing shows, and a tap
  //   only selects none: nothing hidden can be picked.
  // - Pins: a grid over the photo. Touching near a grid point picks it up
  //   as a new pin; touching near a pin (where it was put, or its home)
  //   picks that pin up again. Dragging moves it, live, and brings it to
  //   the top; dragging it back home removes it. Each move is one undo
  //   step. The pin last touched is selected; a tap on an empty spot
  //   selects none.
  //
  // The brush is sized against the screen, as the mask brush is: zoomed in,
  // the same setting pulls a smaller area of the photo.
  import { app } from '../lib/app.svelte';
  import { addPoint, roundCoord } from '../lib/mask';
  import { brushKey, gridAxes, isPin, pinAt, snap, strokeAt, strokeEnd } from '../lib/pull';
  import type { PullMark, PullPin, PullStroke } from '../lib/types';

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
  /** Where the stroke began, and whether the finger has gone far enough from it to be drawing (not tapping). */
  let downAt: { x: number; y: number } | null = null;
  let drawn = $state(false);
  let frame = 0;

  const long = $derived(Math.max(rect.w, rect.h));
  const pinning = $derived(app.pullTool.mode === 'pins');
  /** The next stroke's size on the photo: the selected brush's, or the tool's shrunk by the zoom. */
  const size = $derived(app.strokeSettings().size);
  /** Strokes and pins, as they stand (a live one included). */
  const marks = $derived<PullMark[]>(app.pullTarget?.step.pull ?? []);
  /** The selected brush's key, to light up its markers. */
  const brushSel = $derived(app.selectedBrush?.key ?? null);

  /** How near (in screen pixels) a finger must come to a marker or pin to pick it up. */
  const REACH_TOUCH = 26;
  const REACH_MOUSE = 14;
  /** A press that never strays this far (screen pixels) is a tap. */
  const TAP = 8;
  const reach = (e: PointerEvent) => (e.pointerType === 'mouse' ? REACH_MOUSE : REACH_TOUCH);

  // ---------------------------------------------------------------------
  // Pins

  /** A pin dropped this near its home (screen pixels) is removed. */
  const HOME = 10;

  const axes = $derived(gridAxes(rect.w, rect.h, app.pullTool.grid));
  /**
   * The pin being moved: its place in `base` (the marks when it was
   * touched, a new pin at the end), where the finger holds it, and where
   * it is drawn now (`now`: on top, once it has moved).
   */
  let held = $state<{ i: number; base: PullMark[]; grip: { x: number; y: number }; fresh: boolean; now: number } | null>(null);

  /** A share of the photo, on the stage. */
  const at = (q: [number, number]) => ({ x: rect.x + q[0] * rect.w, y: rect.y + q[1] * rect.h });
  /** A point on the stage, as a share of the photo. */
  const share = (p: { x: number; y: number }): [number, number] => [roundCoord((p.x - rect.x) / rect.w), roundCoord((p.y - rect.y) / rect.h)];

  function pinDown(e: PointerEvent, p: { x: number; y: number }) {
    const current = $state.snapshot(marks) as PullMark[];
    const i = pinAt(current, p, at, reach(e));
    if (i >= 0) {
      const to = at((current[i] as PullPin).to);
      held = { i, base: current, grip: { x: p.x - to.x, y: p.y - to.y }, fresh: false, now: i };
      app.selectMark(i);
    } else {
      const home = snap((p.x - rect.x) / rect.w, (p.y - rect.y) / rect.h, axes);
      const pin: PullPin = { size: app.newPullSize, from: home, to: home };
      const h = at(home);
      held = { i: current.length, base: [...current, pin], grip: { x: p.x - h.x, y: p.y - h.y }, fresh: true, now: current.length };
    }
  }

  /** The marks with the held pin where the finger has it now, on top of the rest (D-68). */
  function moved(p: { x: number; y: number }): PullMark[] {
    if (!held) return [];
    const to = share({ x: p.x - held.grip.x, y: p.y - held.grip.y });
    const pin = { ...(held.base[held.i] as PullPin), to };
    return [...held.base.filter((_, k) => k !== held!.i), pin];
  }

  let latest: PullMark[] | null = null;

  function pinMove(p: { x: number; y: number }) {
    latest = moved(p);
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (!latest) return;
      app.liveMarks(latest);
      if (held) held.now = latest.length - 1;
    });
  }

  function pinUp(p: { x: number; y: number }) {
    if (!held) return;
    const h = held;
    const out = moved(p);
    held = null;
    latest = null;
    const pin = out[out.length - 1] as PullPin;
    const a = at(pin.from);
    const b = at(pin.to);
    const start = at((h.base[h.i] as PullPin).to);
    const dragged = Math.hypot(b.x - start.x, b.y - start.y) > 3;
    if (!h.fresh && !dragged) {
      // Only tapped: it stays where it is in the list, and is selected.
      app.endMarks(null);
      app.selectMark(h.i);
    } else if (Math.hypot(a.x - b.x, a.y - b.y) <= HOME) {
      // Dropped home, the pin goes (a new pin that never left was a tap
      // on an empty spot: nothing is selected).
      app.endMarks(h.fresh ? null : h.base.filter((_, k) => k !== h.i));
      app.pullSel = null;
    } else {
      app.endMarks(out);
      app.selectMark(out.length - 1);
    }
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
    downAt = p;
    drawn = false;
    live = { ...app.strokeSettings(), points: share(p) };
  }

  export function move(e: PointerEvent) {
    if (!root) return;
    const p = onStage(e);
    if (pinning) {
      if (held) pinMove(p);
      return;
    }
    if (e.pointerType === 'mouse' || live) ring = p;
    if (!live || !downAt) return;
    // Until the finger strays a little, it may be a tap on a marker.
    if (!drawn && Math.hypot(p.x - downAt.x, p.y - downAt.y) <= TAP) return;
    drawn = true;
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
    const tapped = !drawn;
    live = null;
    drawn = false;
    if (e.pointerType !== 'mouse') ring = null;
    if (!tapped || !root || !stroke) {
      app.endPull(stroke);
      return;
    }
    // A tap: select the brush whose marker is under it, or none.
    app.endPull(null);
    const i = app.pullTool.markers ? strokeAt($state.snapshot(marks) as PullMark[], onStage(e), at, reach(e)) : -1;
    if (i >= 0) app.selectMark(i);
    else app.pullSel = null;
  }

  /** A second finger came down, or the system took the gesture: nothing changes. */
  export function cancel() {
    cancelAnimationFrame(frame);
    frame = 0;
    if (held) {
      held = null;
      latest = null;
    }
    live = null;
    drawn = false;
    ring = null;
    app.endMarks(null);
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

<div class="painter" bind:this={root} aria-label={pinning ? 'Pull: drag a grid point or a pin' : 'Pull: drag on the photo, or tap a marker'}>
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
      {#each marks as pin, i (i)}
        {#if isPin(pin)}
        {@const a = at(pin.from)}
        {@const b = at(pin.to)}
        {@const chosen = held ? held.now === i : app.selectedPin?.i === i}
        {#if chosen}
          <!-- How far this pin reaches. -->
          <circle class="reach" cx={b.x} cy={b.y} r={(pin.size / 2) * long} />
        {/if}
        <!-- Pins look like stroke markers: plain unless selected, then
             green and larger (Phil, 2026-11-24: hard to see on a phone). -->
        <g class:faded={!chosen}>
          <line class="shade" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
          <line class="tie" x1={a.x} y1={a.y} x2={b.x} y2={b.y} />
          <circle class="home" cx={a.x} cy={a.y} r="4" />
        </g>
        <circle class="marker" class:chosen cx={b.x} cy={b.y} r={chosen ? 10 : 6} />
        {/if}
      {/each}
    </svg>
  {:else if app.pullTool.markers}
    <!-- A marker where each stroke ends; the selected brush's light up,
         with their reach. The stroke being drawn has none yet. -->
    <svg class="pins" width="100%" height="100%">
      {#each marks as m, i (i)}
        {#if !isPin(m) && !(drawn && i === marks.length - 1)}
          {@const q = at(strokeEnd(m))}
          {@const chosen = brushSel !== null && brushKey(marks, i) === brushSel}
          {#if chosen}
            <circle class="reach" cx={q.x} cy={q.y} r={(m.size / 2) * long} />
          {/if}
          <circle class="marker" class:chosen cx={q.x} cy={q.y} r={chosen ? 6 : 4.5} />
        {/if}
      {/each}
    </svg>
  {/if}
  {#if !pinning && ring}
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

  /* An unselected pin's tie and home step back. */
  .faded {
    opacity: 0.55;
  }

  .marker {
    fill: rgb(255 255 255 / 0.9);
    stroke: rgb(0 0 0 / 0.55);
    stroke-width: 1.5;
  }

  .marker.chosen {
    fill: var(--accent);
    stroke: white;
    stroke-width: 2.5;
  }

  .reach {
    fill: none;
    stroke: white;
    stroke-width: 1;
    stroke-dasharray: 4 4;
    opacity: 0.8;
  }
</style>
