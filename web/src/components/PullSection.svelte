<script lang="ts">
  // A pull step's strokes and pins (SPEC 8.2, D-67): how many there are,
  // Draw (pull mode) or Done, and Clear. In pull mode the tool shows here,
  // under the photo, while the photo takes the gestures: Draw (strokes, with
  // size and strength) or Pins (grid points picked up and moved, with size,
  // the grid's spacing and how it shows).
  import Hand from '@lucide/svelte/icons/hand-grab';
  import Eraser from '@lucide/svelte/icons/eraser';
  import Grid from '@lucide/svelte/icons/grid-3x3';
  import { app } from '../lib/app.svelte';
  import { percent } from '../lib/mask';
  import { GRID_MAX, GRID_MIN, pullSizeScale, strokeCount, STRENGTH_MIN, type GridShow, type PullMode } from '../lib/pull';
  import { SLIDER_STEPS } from '../lib/slider';
  import type { StepState } from '../lib/types';

  let { step }: { step: StepState } = $props();

  const count = $derived((step.pull?.length ?? 0) + (step.pin?.length ?? 0));
  const tool = $derived(app.pullTool);

  /** Choose a tool, and start pulling with it if not already. */
  function pick(mode: PullMode) {
    app.setPullTool({ mode });
    if (!active) app.startPulling(step.uid);
  }
  const MODES: [PullMode, string][] = [
    ['draw', 'Draw'],
    ['pins', 'Pins'],
  ];
  const SHOWS: [GridShow, string][] = [
    ['dots', 'Dots'],
    ['lines', 'Lines'],
    ['off', 'Off'],
  ];
  const active = $derived(app.pulling === step.uid && !!app.pullTarget);
  const usable = $derived(step.enabled && !!app.doc);
  const sizeT = $derived(pullSizeScale.toSlider(app.pullTool.size));
</script>

<section class="group" aria-label="Strokes">
  <div class="head">
    <h3 class="section-label">Strokes and pins</h3>
    <span class="dim count">{strokeCount(step.pull?.length ?? 0, step.pin?.length ?? 0)}</span>
  </div>
  <!-- The tools, always in view (Phil, 2026-11-24: Pins hid behind Draw);
       picking one starts pulling with it. -->
  <div class="segmented mode" role="radiogroup" aria-label="Pull tool">
    {#each MODES as [id, label] (id)}
      <button
        role="radio"
        aria-checked={tool.mode === id}
        class:on={tool.mode === id}
        onclick={() => pick(id)}
        disabled={!usable}
        title={usable ? (id === 'draw' ? 'Drag on the photo to pull it' : 'Pick up grid points and move them') : step.enabled ? 'Open a photo first' : 'Turn the step on first'}
      >
        {#if id === 'draw'}<Hand size={16} strokeWidth={1.75} />{:else}<Grid size={16} strokeWidth={1.75} />{/if}
        {label}
      </button>
    {/each}
  </div>
  <div class="actions">
    {#if active}
      <button class="primary" onclick={() => app.stopPulling()} title="Finish pulling (Enter)">Done</button>
    {/if}
    <button class="plain danger" onclick={() => app.clearPull(step.uid)} disabled={!count} title="Remove every stroke and pin (undo brings them back)">
      <Eraser size={16} strokeWidth={1.75} /> Clear
    </button>
  </div>
  {#if active}
    <div class="row">
      <span class="lbl">Size</span>
      <input
        type="range"
        min="0"
        max={SLIDER_STEPS}
        step="1"
        value={sizeT}
        style:--pct="{(sizeT / SLIDER_STEPS) * 100}%"
        oninput={(e) => app.setPullTool({ size: pullSizeScale.fromSlider(Number(e.currentTarget.value)) })}
        aria-label="Brush size"
        title="Brush size ([ and ] keys)"
      />
      <span class="val">{percent(app.pullTool.size)}</span>
    </div>
    {#if tool.mode === 'draw'}
    <div class="row">
      <span class="lbl">Strength</span>
      <input
        type="range"
        min={Math.round(STRENGTH_MIN * 100)}
        max="100"
        step="1"
        value={Math.round(app.pullTool.strength * 100)}
        style:--pct="{((app.pullTool.strength - STRENGTH_MIN) / (1 - STRENGTH_MIN)) * 100}%"
        oninput={(e) => app.setPullTool({ strength: Number(e.currentTarget.value) / 100 })}
        aria-label="Pull strength"
        title="How far the pixels under the middle of the brush follow it"
      />
      <span class="val">{Math.round(app.pullTool.strength * 100)}%</span>
    </div>
    <p class="dim note">
      Drag on the photo to pull it; two fingers zoom and pan. Each stroke is one undo. Size and strength apply to the next stroke.
    </p>
    {:else}
    <div class="row">
      <span class="lbl">Grid</span>
      <input
        type="range"
        min={GRID_MIN}
        max={GRID_MAX}
        step="1"
        value={tool.grid}
        style:--pct="{((tool.grid - GRID_MIN) / (GRID_MAX - GRID_MIN)) * 100}%"
        oninput={(e) => app.setPullTool({ grid: Number(e.currentTarget.value) })}
        aria-label="Grid cells across the photo"
        title="How many grid cells across the photo's longer side"
      />
      <span class="val">{tool.grid}</span>
    </div>
    <div class="row">
      <span class="lbl">Show</span>
      <div class="segmented" role="radiogroup" aria-label="Show the grid as">
        {#each SHOWS as [id, label] (id)}
          <button role="radio" aria-checked={tool.show === id} class:on={tool.show === id} onclick={() => app.setPullTool({ show: id })}>{label}</button>
        {/each}
      </div>
    </div>
    <p class="dim note">
      Drag a grid point to pull it; touch a pin to move it again, or drag it home to remove it. Each move is one undo. Size applies to new pins; the grid only picks points, so changing it moves nothing.
    </p>
    {/if}
  {:else if !count}
    <p class="dim note">Draw to pull pixels along with your finger, or Pins to pick up grid points and move them.</p>
  {/if}
</section>

<style>
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }

  .count {
    font-size: 0.85rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 4px 0;
  }

  .actions .primary {
    gap: 6px;
    border-radius: 999px;
    padding: 6px 16px;
  }

  .actions .plain {
    gap: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .row input[type='range'] {
    flex: 1;
    min-width: 0;
  }

  .lbl {
    flex: none;
    width: 5.2em;
    font-size: 0.9rem;
    font-weight: 500;
  }

  .val {
    flex: none;
    min-width: 4.5ch;
    text-align: right;
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    color: var(--fg-dim);
  }

  .mode {
    align-self: stretch;
    display: flex;
    margin: 4px 0 2px;
  }

  .mode button {
    flex: 1;
  }

  .note {
    font-size: 0.85rem;
    margin: 4px 0 0;
  }
</style>
