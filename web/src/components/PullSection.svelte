<script lang="ts">
  // A pull step's strokes (SPEC 8.2): how many there are, Draw (pull mode)
  // or Done, and Clear. In pull mode the brush settings show here, under
  // the photo, while the photo takes the strokes.
  import Hand from '@lucide/svelte/icons/hand-grab';
  import Eraser from '@lucide/svelte/icons/eraser';
  import { app } from '../lib/app.svelte';
  import { percent } from '../lib/mask';
  import { pullSizeScale, strokeCount, STRENGTH_MIN } from '../lib/pull';
  import { SLIDER_STEPS } from '../lib/slider';
  import type { StepState } from '../lib/types';

  let { step }: { step: StepState } = $props();

  const count = $derived(step.pull?.length ?? 0);
  const active = $derived(app.pulling === step.uid && !!app.pullTarget);
  const usable = $derived(step.enabled && !!app.doc);
  const sizeT = $derived(pullSizeScale.toSlider(app.pullTool.size));
</script>

<section class="group" aria-label="Strokes">
  <div class="head">
    <h3 class="section-label">Strokes</h3>
    <span class="dim count">{strokeCount(count)}</span>
  </div>
  <div class="actions">
    {#if active}
      <button class="primary" onclick={() => app.stopPulling()} title="Finish pulling (Enter)">Done</button>
    {:else}
      <button
        class="primary"
        onclick={() => app.startPulling(step.uid)}
        disabled={!usable}
        title={usable ? 'Drag on the photo to pull it' : step.enabled ? 'Open a photo first' : 'Turn the step on first'}
      >
        <Hand size={16} strokeWidth={1.75} /> Draw
      </button>
    {/if}
    <button class="plain danger" onclick={() => app.clearPull(step.uid)} disabled={!count} title="Remove every stroke (undo brings them back)">
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
  {:else if !count}
    <p class="dim note">Draw on the photo to pull pixels along with your finger.</p>
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

  .note {
    font-size: 0.85rem;
    margin: 4px 0 0;
  }
</style>
