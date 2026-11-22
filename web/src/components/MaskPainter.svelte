<script lang="ts">
  // Painting a step's mask on the preview. Sits exactly over the shown
  // image: the server's mask overlay below, a canvas for the stroke being
  // drawn on top.
  //
  // A stroke is drawn locally while the pointer moves and sent as one mask
  // operation when it is released. The local drawing stays until a render
  // that includes the stroke arrives (tracked by recipe revision), so a
  // stroke never blinks out between release and the server's answer.
  import { app } from '../lib/app.svelte';
  import { MASK_TINT, addPoint, roundCoord } from '../lib/mask';
  import type { MaskOp } from '../lib/types';

  let { rect }: { rect: { x: number; y: number; w: number; h: number } } = $props();

  let canvas = $state<HTMLCanvasElement>();
  const dpr = window.devicePixelRatio || 1;

  /** Operations sent but not yet in a shown render, with their revision. */
  let pending: { rev: number; op: MaskOp }[] = [];
  /** The operation being drawn. */
  let live: MaskOp | null = null;
  /** Brush outline under a mouse pointer, in CSS pixels. */
  let cursor = $state<{ x: number; y: number } | null>(null);

  const long = $derived(Math.max(rect.w, rect.h));
  const brushy = $derived(app.tool.tool === 'brush' || app.tool.tool === 'eraser');

  // Keep the canvas's backing store at device resolution.
  $effect(() => {
    if (!canvas) return;
    canvas.width = Math.max(1, Math.round(rect.w * dpr));
    canvas.height = Math.max(1, Math.round(rect.h * dpr));
    redraw();
  });

  // Once a newer render is on screen, its strokes no longer need drawing.
  $effect(() => {
    const shown = app.paintedRevision;
    pending = pending.filter((p) => p.rev > shown);
    redraw();
  });

  const PAINT = `rgb(${MASK_TINT.join(' ')})`;
  const ERASE = 'rgb(235 219 178)';

  function redraw() {
    const ctx = canvas?.getContext('2d');
    if (!ctx || !canvas) return;
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    for (const p of pending) draw(ctx, p.op);
    if (live) draw(ctx, live);
  }

  function draw(ctx: CanvasRenderingContext2D, op: MaskOp) {
    const W = canvas!.width;
    const H = canvas!.height;
    const L = Math.max(W, H);
    ctx.save();
    if (op.shape === 'stroke') {
      const color = op.mode === 'subtract' ? ERASE : PAINT;
      ctx.strokeStyle = color;
      ctx.fillStyle = color;
      ctx.lineWidth = op.size * L;
      ctx.lineCap = 'round';
      ctx.lineJoin = 'round';
      const p = op.points;
      ctx.beginPath();
      if (p.length === 2) {
        ctx.arc(p[0] * W, p[1] * H, (op.size * L) / 2, 0, 2 * Math.PI);
        ctx.fill();
      } else {
        ctx.moveTo(p[0] * W, p[1] * H);
        for (let i = 2; i < p.length; i += 2) ctx.lineTo(p[i] * W, p[i + 1] * H);
        ctx.stroke();
      }
    } else if (op.shape === 'linear' || op.shape === 'radial') {
      const [ax, ay] = [op.from[0] * W, op.from[1] * H];
      const [bx, by] = [op.to[0] * W, op.to[1] * H];
      ctx.lineWidth = 2 * dpr;
      ctx.strokeStyle = 'white';
      ctx.shadowColor = 'rgb(0 0 0 / 0.6)';
      ctx.shadowBlur = 3 * dpr;
      ctx.beginPath();
      if (op.shape === 'radial') {
        ctx.arc(ax, ay, Math.hypot(bx - ax, by - ay), 0, 2 * Math.PI);
      } else {
        // Lines across the picture at both ends, square to the drag,
        // like Lightroom's linear gradient guides.
        const len = Math.hypot(bx - ax, by - ay) || 1;
        const [nx, ny] = [(-(by - ay) / len) * 2 * L, ((bx - ax) / len) * 2 * L];
        for (const [x, y] of [
          [ax, ay],
          [bx, by],
        ]) {
          ctx.moveTo(x - nx, y - ny);
          ctx.lineTo(x + nx, y + ny);
        }
      }
      ctx.moveTo(ax, ay);
      ctx.lineTo(bx, by);
      ctx.stroke();
    }
    ctx.restore();
  }

  /** Pointer position as fractions of the picture. */
  const norm = (e: PointerEvent): [number, number] => [e.offsetX / rect.w, e.offsetY / rect.h];

  function down(e: PointerEvent) {
    if (e.button !== 0 || !app.masking) return;
    canvas!.setPointerCapture(e.pointerId);
    const [x, y] = norm(e);
    const t = app.tool;
    if (t.tool === 'brush' || t.tool === 'eraser') {
      live = {
        shape: 'stroke',
        mode: t.tool === 'eraser' ? 'subtract' : 'add',
        size: t.size,
        hardness: t.hardness,
        points: [roundCoord(x), roundCoord(y)],
      };
    } else {
      live = { shape: t.tool, mode: 'add', from: [x, y], to: [x, y] };
    }
    redraw();
  }

  function move(e: PointerEvent) {
    if (e.pointerType === 'mouse') cursor = { x: e.offsetX, y: e.offsetY };
    if (!live) return;
    if (live.shape === 'stroke') {
      // Coalesced events carry every sample since the last frame: smoother
      // strokes from fast hands (and Apple Pencil).
      const events = e.getCoalescedEvents?.() ?? [e];
      const minPx = Math.max(1.5, live.size * long * 0.1);
      for (const ev of events.length ? events : [e]) {
        const [x, y] = norm(ev);
        addPoint(live.points, x, y, minPx, rect.w, rect.h);
      }
    } else if (live.shape === 'linear' || live.shape === 'radial') {
      live.to = norm(e);
    }
    redraw();
  }

  function up() {
    const op = live;
    live = null;
    const uid = app.masking;
    if (!op || !uid) return redraw();
    if (op.shape === 'linear' || op.shape === 'radial') {
      // A tap is not a gradient.
      const d = Math.hypot((op.to[0] - op.from[0]) * rect.w, (op.to[1] - op.from[1]) * rect.h);
      if (d < 6) return redraw();
      op.from = [roundCoord(op.from[0]), roundCoord(op.from[1])];
      op.to = [roundCoord(op.to[0]), roundCoord(op.to[1])];
    }
    app.addMaskOp(uid, op);
    pending.push({ rev: app.revision, op });
    redraw();
  }

  function cancel() {
    live = null;
    redraw();
  }
</script>

<div class="painter" style:left="{rect.x}px" style:top="{rect.y}px" style:width="{rect.w}px" style:height="{rect.h}px">
  {#if app.tool.show && app.maskUrl}
    <img class="overlay" src={app.maskUrl} alt="" draggable="false" />
  {/if}
  <canvas
    bind:this={canvas}
    class:brushy
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={cancel}
    onpointerleave={() => (cursor = null)}
    oncontextmenu={(e) => e.preventDefault()}
    aria-label="Paint the mask"
  ></canvas>
  {#if cursor && brushy}
    <div
      class="cursor"
      class:erase={app.tool.tool === 'eraser'}
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

  /* Drawn strokes are opaque on the canvas; the canvas itself is
     translucent, so overlapping segments do not darken. */
  canvas {
    opacity: 0.5;
    touch-action: none;
    cursor: crosshair;
    -webkit-touch-callout: none;
  }

  canvas.brushy {
    cursor: none;
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
