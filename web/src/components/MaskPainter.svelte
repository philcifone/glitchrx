<script lang="ts">
  // Editing a step's mask on the preview. Covers the stage: the server's
  // mask overlay laid over the picture at the bottom, a canvas of fresh
  // brush strokes above it, and a canvas of handles on top. The canvases
  // are the size of the stage, not the picture, so strokes and handles stay
  // sharp, and handles finger-sized, at any zoom.
  //
  // The preview owns the gestures (two fingers always pinch and pan) and
  // hands this component the one-finger ones: down, move, up, cancel.
  //
  // What a gesture does depends on the active component (app.activeOp):
  // - brush: paint or erase a stroke, sent when released. The local drawing
  //   stays until a render that includes it arrives (tracked by recipe
  //   revision), so a stroke never blinks out in between.
  // - linear, radial: drag a handle to adjust it, or drag anywhere else to
  //   place it again. The recipe follows the drag uncommitted, so the
  //   preview keeps up, and the release is one undo step.
  // Linears and radials that are not active show a pin; tapping a pin makes
  // that component active.
  //
  // The brush is sized against the screen, as in Procreate: zoomed in, the
  // same setting paints a finer line on the photo. The size setting is its
  // size at fit.
  import { app } from '../lib/app.svelte';
  import { MASK_TINT, addPoint, dragEnds, grab, grabHandle, pinOf, roundCoord, type Grab } from '../lib/mask';
  import type { MaskOp, Stroke } from '../lib/types';

  let {
    rect,
    stage,
    cursor: cursorStyle = $bindable('default'),
  }: {
    /** Where the picture is drawn on the stage (it may reach past the edges). */
    rect: { x: number; y: number; w: number; h: number };
    /** The stage's size, in CSS pixels. */
    stage: { w: number; h: number };
    /** The mouse cursor the stage should show. */
    cursor?: string;
  } = $props();

  let root = $state<HTMLDivElement>();
  let paint = $state<HTMLCanvasElement>();
  let guides = $state<HTMLCanvasElement>();
  const dpr = window.devicePixelRatio || 1;

  type Shape = Extract<MaskOp, { shape: 'linear' | 'radial' }>;
  const isShape = (op: MaskOp | null | undefined): op is Shape => op?.shape === 'linear' || op?.shape === 'radial';

  const uid = $derived(app.masking);
  const ops = $derived(app.maskTarget?.step.mask?.op ?? []);
  const active = $derived(app.activeOp === null ? null : (ops[app.activeOp] ?? null));
  const brushy = $derived(active?.shape === 'brush');
  /** Linears and radials with their positions, read here so moving one redraws. */
  const shapes = $derived(
    ops.flatMap((op, i) => (isShape(op) ? [{ i, op, from: [...op.from], to: [...op.to] }] : [])),
  );

  /** Strokes sent but not yet in a shown render, with their revision. */
  let pending: { rev: number; stroke: Stroke }[] = [];
  /** The stroke being drawn. */
  let live: Stroke | null = null;
  /** A handle drag in progress, with the shape's ends when it began. */
  let drag: {
    index: number;
    grabbed: Grab;
    start: [number, number];
    from: [number, number];
    to: [number, number];
    moved: boolean;
  } | null = null;
  /** Mouse position (CSS pixels) and what it is over, for the cursor. */
  let cursor = $state<{ x: number; y: number } | null>(null);
  let hover = $state<Grab | 'pin' | null>(null);

  const long = $derived(Math.max(rect.w, rect.h));
  /** The brush's size on the photo: its setting, shrunk by the zoom. */
  const brushSize = $derived(app.tool.size / app.view.z);

  // Keep both canvases' backing stores at device resolution. Only a new
  // stage size reallocates them; zooming and panning just redraw.
  $effect(() => {
    for (const c of [paint, guides]) {
      if (!c) continue;
      c.width = Math.max(1, Math.round(stage.w * dpr));
      c.height = Math.max(1, Math.round(stage.h * dpr));
    }
    redrawStrokes();
    redrawGuides();
  });

  // The picture moved (zoom or pan): draw everything where it now is.
  $effect(() => {
    void [rect.x, rect.y, rect.w, rect.h];
    redrawStrokes();
    redrawGuides();
  });

  // Once a newer render is on screen, its strokes no longer need drawing.
  $effect(() => {
    const shown = app.paintedRevision;
    pending = pending.filter((p) => p.rev > shown);
    redrawStrokes();
  });

  // Handles follow the shapes and the selection.
  $effect(() => {
    void shapes;
    void app.activeOp;
    redrawGuides();
  });

  const PAINT = `rgb(${MASK_TINT.join(' ')})`;
  const ERASE = 'rgb(235 219 178)';
  /** Gruvbox bright green, which reads on photos in either theme. */
  const HANDLE = 'rgb(184 187 38)';
  const SHADOW = 'rgb(0 0 0 / 0.65)';

  function redrawStrokes() {
    const ctx = paint?.getContext('2d');
    if (!ctx || !paint) return;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, paint.width, paint.height);
    // Device pixels, with the picture's top-left corner at the origin.
    ctx.translate(rect.x * dpr, rect.y * dpr);
    for (const p of pending) drawStroke(ctx, p.stroke);
    if (live) drawStroke(ctx, live);
  }

  function drawStroke(ctx: CanvasRenderingContext2D, s: Stroke) {
    const W = rect.w * dpr;
    const H = rect.h * dpr;
    const L = Math.max(W, H);
    const color = s.erase ? ERASE : PAINT;
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = s.size * L;
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    const p = s.points;
    ctx.beginPath();
    if (p.length === 2) {
      ctx.arc(p[0] * W, p[1] * H, (s.size * L) / 2, 0, 2 * Math.PI);
      ctx.fill();
    } else {
      ctx.moveTo(p[0] * W, p[1] * H);
      for (let i = 2; i < p.length; i += 2) ctx.lineTo(p[i] * W, p[i + 1] * H);
      ctx.stroke();
    }
  }

  function redrawGuides() {
    const ctx = guides?.getContext('2d');
    if (!ctx || !guides) return;
    ctx.clearRect(0, 0, guides.width, guides.height);
    ctx.save();
    ctx.scale(dpr, dpr); // draw in CSS pixels
    ctx.translate(rect.x, rect.y); // from the picture's top-left corner
    ctx.shadowColor = SHADOW;
    ctx.shadowBlur = 3;
    for (const s of shapes) {
      if (s.i === app.activeOp) drawHandles(ctx, s.op);
      else drawPin(ctx, s.op);
    }
    ctx.restore();
  }

  /** A round handle: filled for the main one, hollow for the other. */
  function dot(ctx: CanvasRenderingContext2D, x: number, y: number, filled: boolean) {
    ctx.beginPath();
    ctx.arc(x, y, 6, 0, 2 * Math.PI);
    ctx.fillStyle = filled ? HANDLE : 'white';
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = filled ? 'white' : HANDLE;
    ctx.stroke();
  }

  function drawHandles(ctx: CanvasRenderingContext2D, op: Shape) {
    const [w, h] = [rect.w, rect.h];
    const [ax, ay] = [op.from[0] * w, op.from[1] * h];
    const [bx, by] = [op.to[0] * w, op.to[1] * h];
    ctx.strokeStyle = HANDLE;
    ctx.lineWidth = 2;
    ctx.beginPath();
    if (op.shape === 'radial') {
      ctx.arc(ax, ay, Math.hypot(bx - ax, by - ay), 0, 2 * Math.PI);
      ctx.stroke();
    } else {
      // Lines across the picture at both ends, square to the drag, like
      // Lightroom's linear gradient guides: solid where it is full, dashed
      // where it has faded out.
      const len = Math.hypot(bx - ax, by - ay) || 1;
      const [nx, ny] = [(-(by - ay) / len) * 2 * long, ((bx - ax) / len) * 2 * long];
      ctx.moveTo(ax - nx, ay - ny);
      ctx.lineTo(ax + nx, ay + ny);
      ctx.stroke();
      ctx.beginPath();
      ctx.setLineDash([6, 5]);
      ctx.moveTo(bx - nx, by - ny);
      ctx.lineTo(bx + nx, by + ny);
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.stroke();
      ctx.setLineDash([]);
    }
    dot(ctx, ax, ay, true);
    dot(ctx, bx, by, false);
  }

  function drawPin(ctx: CanvasRenderingContext2D, op: Shape) {
    const [x, y] = pinOf(op, rect.w, rect.h);
    ctx.beginPath();
    ctx.arc(x, y, 7, 0, 2 * Math.PI);
    ctx.fillStyle = 'white';
    ctx.fill();
    ctx.shadowBlur = 0;
    ctx.beginPath();
    ctx.arc(x, y, 3, 0, 2 * Math.PI);
    ctx.fillStyle = 'rgb(40 40 40)';
    ctx.fill();
    ctx.shadowBlur = 3;
  }

  /** The inactive component whose pin is under `p`, if any. */
  function pinAt(p: [number, number], tol: number): number | null {
    let best: number | null = null;
    let bestD = Math.max(tol, 10);
    for (const s of shapes) {
      if (s.i === app.activeOp) continue;
      const [x, y] = pinOf(s.op, rect.w, rect.h);
      const d = Math.hypot(p[0] - x, p[1] - y);
      if (d < bestD) [best, bestD] = [s.i, d];
    }
    return best;
  }

  /** Hit tolerance: fingers are bigger than mouse pointers. */
  const tolerance = (e: PointerEvent) => (e.pointerType === 'touch' ? 22 : 10);
  /** The stage point of an event. */
  function onStage(e: PointerEvent): { x: number; y: number } {
    const b = root!.getBoundingClientRect();
    return { x: e.clientX - b.left, y: e.clientY - b.top };
  }
  /** The event's position from the picture's top-left corner, in CSS pixels. */
  function at(e: PointerEvent): [number, number] {
    const p = onStage(e);
    return [p.x - rect.x, p.y - rect.y];
  }

  /** A finger or the mouse button went down on the stage. */
  export function down(e: PointerEvent) {
    if (e.button !== 0 || !uid || !root) return;
    const p = at(e);
    const tol = tolerance(e);
    if (active?.shape === 'brush') {
      live = {
        ...(app.tool.tool === 'erase' ? { erase: true } : {}),
        size: roundSize(brushSize),
        hardness: app.tool.hardness,
        points: [roundCoord(p[0] / rect.w), roundCoord(p[1] / rect.h)],
      };
      redrawStrokes();
      return;
    }
    // The active shape's handle dots first, then other components' pins,
    // then the rest of the active shape (its ring, lines or inside), then
    // drawing it again.
    const handle = isShape(active) ? grabHandle(active, p, rect.w, rect.h, tol) : null;
    if (!handle) {
      const pin = pinAt(p, tol);
      if (pin !== null) return app.selectComponent(uid, pin);
    }
    if (!isShape(active) || app.activeOp === null) return;
    const grabbed = handle ?? grab(active, p, rect.w, rect.h, tol);
    drag = { index: app.activeOp, grabbed, start: p, from: [...active.from], to: [...active.to], moved: false };
  }

  /** A move, with or without a button down (the mouse shows the brush). */
  export function move(e: PointerEvent) {
    if (!root) return;
    const p = at(e);
    if (e.pointerType === 'mouse') {
      const s = onStage(e);
      cursor = { x: s.x, y: s.y };
      if (!live && !drag) hover = hoverAt(p);
    }
    if (live) {
      // Coalesced events carry every sample since the last frame: smoother
      // strokes from fast hands (and Apple Pencil).
      const events = e.getCoalescedEvents?.() ?? [e];
      const minPx = Math.max(1.5, live.size * long * 0.1);
      for (const ev of events.length ? events : [e]) {
        const [x, y] = at(ev);
        addPoint(live.points, x / rect.w, y / rect.h, minPx, rect.w, rect.h);
      }
      redrawStrokes();
    } else if (drag && uid) {
      // A few pixels of wobble is still a tap.
      if (!drag.moved && Math.hypot(p[0] - drag.start[0], p[1] - drag.start[1]) < 4) return;
      drag.moved = true;
      const ends = dragEnds(drag.grabbed, drag.from, drag.to, drag.start, p, rect.w, rect.h);
      app.updateMaskOp(uid, drag.index, ends, false);
    }
  }

  export function up(e: PointerEvent) {
    if (!root) return;
    if (live) {
      const stroke = live;
      live = null;
      if (uid && app.activeOp !== null) {
        // A tap on a pin picks that component rather than leaving a dab.
        const pin = stroke.points.length === 2 ? pinAt(at(e), tolerance(e)) : null;
        if (pin !== null) app.selectComponent(uid, pin);
        else {
          app.addStroke(uid, app.activeOp, stroke);
          pending.push({ rev: app.revision, stroke });
        }
      }
      redrawStrokes();
    } else if (drag && uid) {
      const d = drag;
      drag = null;
      if (!d.moved) return;
      // Drawing a new placement needs a real drag; a short one puts it back.
      const p = at(e);
      if (d.grabbed === 'draw' && Math.hypot(p[0] - d.start[0], p[1] - d.start[1]) < 8) {
        app.updateMaskOp(uid, d.index, { from: d.from, to: d.to }, false);
      } else {
        app.commit();
      }
    }
  }

  /** The gesture ended without a release (a second finger came down, or the system took it). */
  export function cancel() {
    live = null;
    redrawStrokes();
    if (drag?.moved && uid) app.updateMaskOp(uid, drag.index, { from: drag.from, to: drag.to }, false);
    drag = null;
  }

  /** What the mouse would grab here, for the cursor. */
  function hoverAt(p: [number, number]): Grab | 'pin' | null {
    if (brushy) return null;
    const handle = isShape(active) ? grabHandle(active, p, rect.w, rect.h, 10) : null;
    if (handle) return handle;
    if (pinAt(p, 10) !== null) return 'pin';
    return isShape(active) ? grab(active, p, rect.w, rect.h, 10) : null;
  }

  /** The mouse left the stage. */
  export function leave() {
    cursor = null;
  }

  /** Strokes keep their size to 1/100,000 of the picture, as positions do. */
  const roundSize = (v: number) => Math.max(0.00001, Math.round(v * 100000) / 100000);

  // The stage shows the cursor (the canvases take no events themselves).
  $effect(() => {
    cursorStyle = brushy ? 'none' : hover ? CURSOR[hover] : 'default';
  });

  const CURSOR: Record<Grab | 'pin', string> = {
    edge: 'grab',
    from: 'grab',
    to: 'grab',
    move: 'move',
    pin: 'pointer',
    draw: 'crosshair',
  };
</script>

<div class="painter" bind:this={root}>
  {#if app.tool.show && app.maskUrl}
    <img
      class="overlay"
      src={app.maskUrl}
      alt=""
      draggable="false"
      style:left="{rect.x}px"
      style:top="{rect.y}px"
      style:width="{rect.w}px"
      style:height="{rect.h}px"
    />
  {/if}
  <canvas class="strokes" bind:this={paint}></canvas>
  <canvas bind:this={guides} aria-label="Edit the mask"></canvas>
  {#if cursor && brushy}
    <div
      class="cursor"
      class:erase={app.tool.tool === 'erase'}
      style:left="{cursor.x}px"
      style:top="{cursor.y}px"
      style:width="{brushSize * long}px"
      style:height="{brushSize * long}px"
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

  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .overlay {
    position: absolute;
    max-width: none;
    opacity: 0.45;
    user-select: none;
    -webkit-user-select: none;
  }

  /* Strokes are drawn opaque on a translucent canvas, so overlapping
     segments do not darken. */
  canvas.strokes {
    opacity: 0.5;
    pointer-events: none;
  }

  .cursor {
    position: absolute;
    transform: translate(-50%, -50%);
    border-radius: 50%;
    border: 1.5px solid white;
    box-shadow:
      0 0 0 1px rgb(0 0 0 / 0.5),
      inset 0 0 0 1px rgb(0 0 0 / 0.5);
    pointer-events: none;
  }

  .cursor.erase {
    border-style: dashed;
  }
</style>
