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
  // - rectangle: corners resize against the opposite corner, the handle
  //   above the top edge turns it (shift: 15 degree steps), inside moves it,
  //   and a drag outside draws it again at the same angle.
  // - pen: while drawing (app.penPlacing), each tap adds a point and a drag
  //   from it pulls out a handle that bends the sides into a curve; tapping
  //   the first point closes it. Afterwards points and the picked point's
  //   handles drag, a tap on a side adds a point, and inside moves it all.
  // Shapes that are not active show a pin; tapping a pin makes that
  // component active.
  //
  // The brush is sized against the screen, as in Procreate: zoomed in, the
  // same setting paints a finer line on the photo. The size setting is its
  // size at fit.
  import { app } from '../lib/app.svelte';
  import { MASK_TINT, addPoint, dragEnds, grab, grabHandle, pinOf, roundCoord, type Grab } from '../lib/mask';
  import {
    anchorPx,
    dragRect,
    grabPath,
    grabRect,
    grabRectHandle,
    handlesPx,
    insertPoint,
    moveAll,
    moveHandle,
    movePoint,
    pathPin,
    pointAt,
    pullHandle,
    rectGeom,
    sides,
    type PenGrab,
    type Pt,
    type RectGrab,
    type RectOp,
    type Side,
  } from '../lib/shapes';
  import type { MaskOp, Stroke } from '../lib/types';

  let {
    rect,
    canvas = rect,
    stage,
    cursor: cursorStyle = $bindable('default'),
  }: {
    /**
     * Where the whole photo sits on the stage (it may reach past the edges,
     * and past a crop): every mask position is a share of it (D-52).
     */
    rect: { x: number; y: number; w: number; h: number };
    /** Where the cropped canvas, and so the server's mask view, is drawn. */
    canvas?: { x: number; y: number; w: number; h: number };
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
  type PathOp = Extract<MaskOp, { shape: 'path' }>;
  /** Components drawn on the photo with handles (and a pin when inactive). */
  type Handled = Shape | RectOp | PathOp;
  const isShape = (op: MaskOp | null | undefined): op is Shape => op?.shape === 'linear' || op?.shape === 'radial';
  const isHandled = (op: MaskOp | null | undefined): op is Handled =>
    isShape(op) || op?.shape === 'rect' || op?.shape === 'path';
  /** A plain copy of a path's points (the recipe's are reactive proxies). */
  const copyPoints = (op: PathOp) => $state.snapshot(op.points) as number[][];

  const uid = $derived(app.masking);
  const ops = $derived(app.maskTarget?.step.mask?.op ?? []);
  const active = $derived(app.activeOp === null ? null : (ops[app.activeOp] ?? null));
  const brushy = $derived(active?.shape === 'brush');
  /** Components with handles, with their geometry read here (sig) so a change redraws. */
  const shapes = $derived(ops.flatMap((op, i) => (isHandled(op) ? [{ i, op, sig: JSON.stringify(op) }] : [])));

  /** Strokes sent but not yet in a shown render, with their revision. */
  let pending: { rev: number; stroke: Stroke }[] = [];
  /** The stroke being drawn. */
  let live: Stroke | null = null;
  /**
   * A handle drag in progress, with the shape as it was when it began (so
   * each move starts from there, and a cancel puts it back).
   */
  type Drag = { index: number; start: Pt; moved: boolean } & (
    | { kind: 'ends'; grabbed: Grab; from: [number, number]; to: [number, number] }
    | { kind: 'rect'; grab: RectGrab; orig: RectOp }
    | { kind: 'pen'; grab: PenGrab | { kind: 'place'; i: number }; orig: number[][] }
  );
  let drag: Drag | null = null;
  /** Mouse position (CSS pixels), and the cursor for what it is over. */
  let cursor = $state<{ x: number; y: number } | null>(null);
  let hover = $state('default');

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

  // Handles follow the shapes, the selection and the pen's state.
  $effect(() => {
    void shapes;
    void app.activeOp;
    void app.penPoint;
    void app.penPlacing;
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
      if (s.i !== app.activeOp) drawPin(ctx, s.op);
      else if (s.op.shape === 'rect') drawRect(ctx, s.op);
      else if (s.op.shape === 'path') drawPath(ctx, s.op);
      else drawHandles(ctx, s.op);
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

  function drawRect(ctx: CanvasRenderingContext2D, op: RectOp) {
    const g = rectGeom(op, rect.w, rect.h);
    ctx.strokeStyle = HANDLE;
    ctx.lineWidth = 2;
    ctx.beginPath();
    g.corners.forEach(([x, y], k) => (k ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    ctx.closePath();
    ctx.moveTo(...g.top);
    ctx.lineTo(...g.rotate);
    ctx.stroke();
    for (const [x, y] of g.corners) dot(ctx, x, y, false);
    dot(ctx, g.rotate[0], g.rotate[1], true);
  }

  /** Trace sides as one line (curves as Beziers, as the server fills them). */
  function trace(ctx: CanvasRenderingContext2D, ss: Side[]) {
    if (!ss.length) return;
    ctx.beginPath();
    ctx.moveTo(...ss[0].a);
    for (const s of ss) {
      if (s.curved) ctx.bezierCurveTo(...s.b, ...s.c, ...s.d);
      else ctx.lineTo(...s.d);
    }
    ctx.stroke();
  }

  function drawPath(ctx: CanvasRenderingContext2D, op: PathOp) {
    const [w, h] = [rect.w, rect.h];
    const pts = op.points;
    const placing = app.penPlacing;
    ctx.strokeStyle = HANDLE;
    ctx.lineWidth = 2;
    trace(ctx, sides(pts, w, h, !placing));
    // While drawing, the side that will close it, dashed.
    if (placing && pts.length >= 2) {
      ctx.setLineDash([6, 5]);
      trace(ctx, sides(pts, w, h).slice(-1));
      ctx.setLineDash([]);
    }
    // The picked point's handles.
    const i = app.penPoint;
    const hs = i !== null && pts[i] ? handlesPx(pts[i], w, h) : null;
    if (hs && i !== null) {
      const a = anchorPx(pts[i], w, h);
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(...hs.in);
      ctx.lineTo(...a);
      ctx.lineTo(...hs.out);
      ctx.stroke();
      for (const [x, y] of [hs.in, hs.out]) {
        ctx.beginPath();
        ctx.arc(x, y, 4.5, 0, 2 * Math.PI);
        ctx.fillStyle = HANDLE;
        ctx.fill();
      }
    }
    // Points as squares, as pen tools draw them; the first is larger while
    // it can close the path.
    pts.forEach((q, k) => {
      const [x, y] = anchorPx(q, w, h);
      const r = placing && k === 0 && pts.length >= 3 ? 7.5 : 5;
      ctx.fillStyle = k === i ? HANDLE : 'white';
      ctx.strokeStyle = k === i ? 'white' : HANDLE;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.rect(x - r, y - r, 2 * r, 2 * r);
      ctx.fill();
      ctx.stroke();
    });
  }

  /** Where an inactive component's pin sits, if it has one yet. */
  function pinFor(op: Handled): Pt | null {
    if (op.shape === 'rect') return rectGeom(op, rect.w, rect.h).centre;
    if (op.shape === 'path') return pathPin(op.points, rect.w, rect.h);
    return pinOf(op, rect.w, rect.h);
  }

  function drawPin(ctx: CanvasRenderingContext2D, op: Handled) {
    const at = pinFor(op);
    if (!at) return;
    const [x, y] = at;
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
      const pin = s.i === app.activeOp ? null : pinFor(s.op);
      if (!pin) continue;
      const [x, y] = pin;
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
    const index = app.activeOp;
    if (active?.shape === 'path' && index !== null) return penDown(active, index, p, tol);
    // The active shape's handle dots first, then other components' pins,
    // then the rest of the active shape (its ring, lines or inside), then
    // drawing it again.
    const [w, h] = [rect.w, rect.h];
    const handle = isShape(active)
      ? grabHandle(active, p, w, h, tol)
      : active?.shape === 'rect'
        ? grabRectHandle(active, p, w, h, tol)
        : null;
    if (!handle) {
      const pin = pinAt(p, tol);
      if (pin !== null) return app.selectComponent(uid, pin);
    }
    if (index === null) return;
    const base = { index, start: p, moved: false };
    if (isShape(active)) {
      const grabbed = (handle as Grab | null) ?? grab(active, p, w, h, tol);
      drag = { ...base, kind: 'ends', grabbed, from: [...active.from], to: [...active.to] };
    } else if (active?.shape === 'rect') {
      const g = (handle as RectGrab | null) ?? grabRect(active, p, w, h, tol);
      drag = { ...base, kind: 'rect', grab: g, orig: $state.snapshot(active) as RectOp };
    }
  }

  /** A press while a pen path is active. */
  function penDown(op: PathOp, index: number, p: Pt, tol: number) {
    const [w, h] = [rect.w, rect.h];
    const pts = copyPoints(op);
    const base = { kind: 'pen' as const, index, start: p, moved: false, orig: pts };
    if (app.penPlacing) {
      const hit = pointAt(pts, p, w, h, tol);
      // Tapping the first point closes the path.
      if (hit === 0 && pts.length >= 3) {
        app.finishPen();
        return;
      }
      if (hit !== null) {
        app.penPoint = hit;
        drag = { ...base, grab: { kind: 'point', i: hit } };
        return;
      }
      // A new point; a drag from it pulls out its handle (see move).
      app.setPenPoints([...pts, [roundCoord(p[0] / w), roundCoord(p[1] / h)]], false);
      app.penPoint = pts.length;
      drag = { ...base, grab: { kind: 'place', i: pts.length } };
      return;
    }
    const g = grabPath(pts, p, w, h, tol, app.penPoint);
    // Points and handles come before other components' pins; a side or
    // the inside after them.
    if (g.kind !== 'point' && g.kind !== 'handle') {
      const pin = pinAt(p, tol);
      if (pin !== null) return app.selectComponent(uid!, pin);
    }
    if (g.kind === 'none') {
      app.penPoint = null;
      return;
    }
    if (g.kind === 'point' || g.kind === 'handle') app.penPoint = g.i;
    drag = { ...base, grab: g };
  }

  /** The path's points while a pen drag is at `p`. */
  function penDragged(d: Extract<Drag, { kind: 'pen' }>, p: Pt): number[][] {
    const [w, h] = [rect.w, rect.h];
    const [dx, dy] = [p[0] - d.start[0], p[1] - d.start[1]];
    const g = d.grab;
    switch (g.kind) {
      case 'place':
        return pullHandle([...d.orig, [roundCoord(d.start[0] / w), roundCoord(d.start[1] / h)]], g.i, p, w, h);
      case 'point':
        return movePoint(d.orig, g.i, dx, dy, w, h);
      case 'handle':
        return moveHandle(d.orig, g.i, g.end, p, w, h);
      default:
        // A side or the inside: the whole path moves.
        return moveAll(d.orig, dx, dy, w, h);
    }
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
      if (drag.kind === 'ends') {
        const ends = dragEnds(drag.grabbed, drag.from, drag.to, drag.start, p, rect.w, rect.h);
        app.updateMaskOp(uid, drag.index, ends, false);
      } else if (drag.kind === 'rect') {
        app.updateMaskOp(uid, drag.index, dragRect(drag.grab, drag.orig, drag.start, p, rect.w, rect.h, e.shiftKey), false);
      } else {
        app.setPenPoints(penDragged(drag, p), false);
      }
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
      if (d.kind === 'pen') return penUp(d);
      if (!d.moved) return;
      // Drawing a new placement needs a real drag; a short one puts it back.
      const p = at(e);
      const short = Math.hypot(p[0] - d.start[0], p[1] - d.start[1]) < 8;
      if (d.kind === 'ends' && d.grabbed === 'draw' && short) {
        app.updateMaskOp(uid, d.index, { from: d.from, to: d.to }, false);
      } else if (d.kind === 'rect' && d.grab.kind === 'draw' && short) {
        app.updateMaskOp(uid, d.index, d.orig, false);
      } else {
        app.commit();
      }
    }
  }

  function penUp(d: Extract<Drag, { kind: 'pen' }>) {
    // A placed point is one undo step, handle and all.
    if (d.grab.kind === 'place' || d.moved) return app.commit();
    // A tap on a side adds a point there.
    if (d.grab.kind === 'side') {
      app.setPenPoints(insertPoint(d.orig, d.grab.i, d.grab.t, rect.w, rect.h));
      app.penPoint = d.grab.i + 1;
    }
  }

  /** The gesture ended without a release (a second finger came down, or the system took it). */
  export function cancel() {
    live = null;
    redrawStrokes();
    if (drag && uid) {
      if (drag.kind === 'pen' && (drag.moved || drag.grab.kind === 'place')) app.setPenPoints(drag.orig, false);
      else if (drag.kind === 'rect' && drag.moved) app.updateMaskOp(uid, drag.index, drag.orig, false);
      else if (drag.kind === 'ends' && drag.moved) app.updateMaskOp(uid, drag.index, { from: drag.from, to: drag.to }, false);
    }
    drag = null;
  }

  /** The cursor for what the mouse would grab here. */
  function hoverAt(p: Pt): string {
    const [w, h] = [rect.w, rect.h];
    if (brushy) return 'none';
    if (active?.shape === 'path') {
      if (app.penPlacing) {
        const hit = pointAt(active.points, p, w, h, 10);
        return hit === 0 && active.points.length >= 3 ? 'pointer' : hit !== null ? 'grab' : 'crosshair';
      }
      const g = grabPath(active.points, p, w, h, 10, app.penPoint);
      if (g.kind === 'point' || g.kind === 'handle') return 'grab';
      if (pinAt(p, 10) !== null) return 'pointer';
      return g.kind === 'side' ? 'copy' : g.kind === 'move' ? 'move' : 'default';
    }
    const handle = isShape(active) ? grabHandle(active, p, w, h, 10) : null;
    if (handle) return CURSOR[handle];
    if (active?.shape === 'rect' && grabRectHandle(active, p, w, h, 10)) return 'grab';
    if (pinAt(p, 10) !== null) return 'pointer';
    if (active?.shape === 'rect') return CURSOR[grabRect(active, p, w, h, 10).kind === 'move' ? 'move' : 'draw'];
    return isShape(active) ? CURSOR[grab(active, p, w, h, 10)] : 'default';
  }

  /** The mouse left the stage. */
  export function leave() {
    cursor = null;
  }

  /** Strokes keep their size to 1/100,000 of the picture, as positions do. */
  const roundSize = (v: number) => Math.max(0.00001, Math.round(v * 100000) / 100000);

  // The stage shows the cursor (the canvases take no events themselves).
  $effect(() => {
    cursorStyle = brushy ? 'none' : hover;
  });

  const CURSOR: Record<Grab, string> = {
    edge: 'grab',
    from: 'grab',
    to: 'grab',
    move: 'move',
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
      style:left="{canvas.x}px"
      style:top="{canvas.y}px"
      style:width="{canvas.w}px"
      style:height="{canvas.h}px"
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
