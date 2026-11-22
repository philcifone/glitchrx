<script lang="ts">
  // Editing a step's mask on the preview. Sits exactly over the shown
  // image: the server's mask overlay at the bottom, a canvas of fresh brush
  // strokes above it, and a canvas of handles on top, which takes the
  // gestures.
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
  import { app } from '../lib/app.svelte';
  import { MASK_TINT, addPoint, dragEnds, grab, grabHandle, pinOf, roundCoord, type Grab } from '../lib/mask';
  import type { MaskOp, Stroke } from '../lib/types';

  let { rect }: { rect: { x: number; y: number; w: number; h: number } } = $props();

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

  // Keep both canvases' backing stores at device resolution.
  $effect(() => {
    for (const c of [paint, guides]) {
      if (!c) continue;
      c.width = Math.max(1, Math.round(rect.w * dpr));
      c.height = Math.max(1, Math.round(rect.h * dpr));
    }
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
    ctx.clearRect(0, 0, paint.width, paint.height);
    for (const p of pending) drawStroke(ctx, p.stroke);
    if (live) drawStroke(ctx, live);
  }

  function drawStroke(ctx: CanvasRenderingContext2D, s: Stroke) {
    const W = paint!.width;
    const H = paint!.height;
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
  const at = (e: PointerEvent): [number, number] => [e.offsetX, e.offsetY];

  function down(e: PointerEvent) {
    if (e.button !== 0 || !uid) return;
    guides!.setPointerCapture(e.pointerId);
    const p = at(e);
    const tol = tolerance(e);
    if (active?.shape === 'brush') {
      live = {
        ...(app.tool.tool === 'erase' ? { erase: true } : {}),
        size: app.tool.size,
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

  function move(e: PointerEvent) {
    const p = at(e);
    if (e.pointerType === 'mouse') {
      cursor = { x: p[0], y: p[1] };
      if (!live && !drag) hover = hoverAt(p);
    }
    if (live) {
      // Coalesced events carry every sample since the last frame: smoother
      // strokes from fast hands (and Apple Pencil).
      const events = e.getCoalescedEvents?.() ?? [e];
      const minPx = Math.max(1.5, live.size * long * 0.1);
      for (const ev of events.length ? events : [e]) {
        addPoint(live.points, ev.offsetX / rect.w, ev.offsetY / rect.h, minPx, rect.w, rect.h);
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

  function up(e: PointerEvent) {
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

  function cancel() {
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

  const CURSOR: Record<Grab | 'pin', string> = {
    edge: 'grab',
    from: 'grab',
    to: 'grab',
    move: 'move',
    pin: 'pointer',
    draw: 'crosshair',
  };
</script>

<div class="painter" style:left="{rect.x}px" style:top="{rect.y}px" style:width="{rect.w}px" style:height="{rect.h}px">
  {#if app.tool.show && app.maskUrl}
    <img class="overlay" src={app.maskUrl} alt="" draggable="false" />
  {/if}
  <canvas class="strokes" bind:this={paint}></canvas>
  <canvas
    bind:this={guides}
    style:cursor={brushy ? 'none' : hover ? CURSOR[hover] : 'default'}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={cancel}
    onpointerleave={() => (cursor = null)}
    oncontextmenu={(e) => e.preventDefault()}
    aria-label="Edit the mask"
  ></canvas>
  {#if cursor && brushy}
    <div
      class="cursor"
      class:erase={app.tool.tool === 'erase'}
      style:left="{cursor.x}px"
      style:top="{cursor.y}px"
      style:width="{app.tool.size * long}px"
      style:height="{app.tool.size * long}px"
    ></div>
  {/if}
</div>

<style>
  .painter {
    position: absolute;
    overflow: hidden;
  }

  .overlay,
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .overlay {
    opacity: 0.45;
    pointer-events: none;
    user-select: none;
    -webkit-user-select: none;
  }

  /* Strokes are drawn opaque on a translucent canvas, so overlapping
     segments do not darken. */
  canvas.strokes {
    opacity: 0.5;
    pointer-events: none;
  }

  canvas {
    touch-action: none;
    -webkit-touch-callout: none;
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
