<script lang="ts">
  // A pull step's strokes and pins (SPEC 8.2, D-67, D-68). The card looks
  // the same whether or not the photo is being pulled (Phil, 2026-11-24):
  // the tools (Draw or Pins; picking one starts pulling), what the sliders
  // change (the selected brush or pin, or the next one) and New, then the
  // tool's settings, then "Delete this pin" (or brush) while one is
  // selected. There is no Delete all. Done is in the top bar only.
  import Hand from '@lucide/svelte/icons/hand-grab';
  import Grid from '@lucide/svelte/icons/grid-3x3';
  import Plus from '@lucide/svelte/icons/plus';
  import Trash from '@lucide/svelte/icons/trash-2';
  import { app } from '../lib/app.svelte';
  import { percent } from '../lib/mask';
  import { GRID_MAX, GRID_MIN, pullSizeScale, strokeCount, STRENGTH_MIN, type GridShow, type PullMode } from '../lib/pull';
  import { SLIDER_STEPS } from '../lib/slider';
  import type { StepState } from '../lib/types';

  let { step }: { step: StepState } = $props();

  const tool = $derived(app.pullTool);
  const active = $derived(app.pulling === step.uid && !!app.pullTarget);
  const usable = $derived(step.enabled && !!app.doc);
  const drawing = $derived(tool.mode === 'draw');
  // Selections belong to the step being pulled.
  const brush = $derived(active ? app.selectedBrush : null);
  const pin = $derived(active ? app.selectedPin : null);
  const selected = $derived(!!brush || !!pin);

  /** What the sliders show: the selection's settings, else the tool's. */
  const size = $derived(brush?.first.size ?? pin?.pin.size ?? tool.size);
  const strength = $derived(brush?.first.strength ?? tool.strength);
  /** The size on its slider's scale. */
  const t = $derived(pullSizeScale.toSlider(size));

  /** What the sliders change, in words. */
  const target = $derived.by(() => {
    if (brush) return brush.strokes.length === 1 ? 'Selected brush, 1 stroke' : `Selected brush, ${brush.strokes.length} strokes`;
    if (pin) return 'Selected pin';
    return drawing ? 'New brush' : 'New pin';
  });

  /** Choose a tool, and start pulling with it if not already. */
  function pick(mode: PullMode) {
    if (mode !== tool.mode) app.pullSel = null;
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
  const why = $derived(usable ? '' : step.enabled ? 'Open a photo first' : 'Turn the step on first');
</script>

<section class="group" aria-label="Strokes and pins">
  <div class="head">
    <h3 class="section-label">Strokes and pins</h3>
    <span class="dim count">{strokeCount(step.pull ?? [])}</span>
  </div>
  <!-- The tools, always in view; picking one starts pulling with it. -->
  <div class="segmented mode" role="radiogroup" aria-label="Pull tool">
    {#each MODES as [id, label] (id)}
      <button
        role="radio"
        aria-checked={tool.mode === id}
        class:on={tool.mode === id}
        onclick={() => pick(id)}
        disabled={!usable}
        title={why || (id === 'draw' ? 'Drag on the photo to pull it' : 'Pick up grid points and move them')}
      >
        {#if id === 'draw'}<Hand size={16} strokeWidth={1.75} />{:else}<Grid size={16} strokeWidth={1.75} />{/if}
        {label}
      </button>
    {/each}
  </div>
  <!-- What the sliders below change; New starts the next brush or pin. -->
  <div class="target">
    <span class="what" class:sel={selected}>{target}</span>
    <!-- Labelled, unlike the icons around it: the add-effect button is a
         plus too, and sits nearby. -->
    <button
      class="plain new"
      onclick={() => app.newMark(step.uid)}
      disabled={!usable || (active && !selected)}
      title={why || (drawing ? 'New brush: the sliders set the next stroke' : 'New pin: the sliders set the next pin')}
      aria-label={drawing ? 'New brush' : 'New pin'}
    >
      <Plus size={16} strokeWidth={1.75} /> New
    </button>
  </div>
  <div class="row">
    <span class="lbl">Size</span>
    <input
      type="range"
      min="0"
      max={SLIDER_STEPS}
      step="1"
      value={t}
      style:--pct="{(t / SLIDER_STEPS) * 100}%"
      oninput={(e) => app.adjustPull({ size: pullSizeScale.fromSlider(Number(e.currentTarget.value)) }, false)}
      onchange={(e) => app.adjustPull({ size: pullSizeScale.fromSlider(Number(e.currentTarget.value)) }, true)}
      aria-label={brush ? 'Size of the selected brush' : pin ? 'Size of the selected pin' : 'Size of the next one'}
      title="Size ([ and ] keys)"
    />
    <span class="val">{percent(size)}</span>
  </div>
  {#if drawing}
    <div class="row">
      <span class="lbl">Strength</span>
      <input
        type="range"
        min={Math.round(STRENGTH_MIN * 100)}
        max="100"
        step="1"
        value={Math.round(strength * 100)}
        style:--pct="{((strength - STRENGTH_MIN) / (1 - STRENGTH_MIN)) * 100}%"
        oninput={(e) => app.adjustPull({ strength: Number(e.currentTarget.value) / 100 }, false)}
        onchange={(e) => app.adjustPull({ strength: Number(e.currentTarget.value) / 100 }, true)}
        aria-label={brush ? 'Strength of the selected brush' : 'Strength of the next brush'}
        title="How far the pixels under the middle of the brush follow it"
      />
      <span class="val">{Math.round(strength * 100)}%</span>
    </div>
    <label class="row">
      <span class="lbl">Markers</span>
      <span class="grow"></span>
      <input
        class="switch"
        type="checkbox"
        checked={tool.markers}
        onchange={(e) => app.setPullTool({ markers: e.currentTarget.checked })}
        title="A dot where each stroke ends; tap one to select its brush"
      />
    </label>
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
  {/if}
  <!-- Only the one selected; there is no Delete all (Phil, 2026-11-24). -->
  {#if selected}
    <button class="plain danger delete" onclick={() => app.deleteSelected()} title={brush ? `Remove the ${brush.strokes.length === 1 ? 'stroke' : `${brush.strokes.length} strokes`} of this brush (undo brings them back)` : 'Remove this pin (undo brings it back)'}>
      <Trash size={16} strokeWidth={1.75} />
      {brush ? 'Delete this brush' : 'Delete this pin'}
    </button>
  {/if}
  <p class="dim note">
    {#if !active}
      Tap Draw or Pins to pull the photo.
    {:else if drawing}
      Drag on the photo to pull it. Strokes join the selected brush; {tool.markers ? "tap a stroke's marker to select its brush, or New" : 'New'} for a new one. Whatever you do last goes on top.
    {:else}
      Drag a grid point to pull it; drag a pin to move it again (it comes to the top), or back home to remove it. Tap a pin to select it, or New for a new one.
    {/if}
  </p>
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

  .target {
    display: flex;
    align-items: center;
    gap: 4px;
    min-height: 36px;
    margin: 2px 0;
  }

  .what {
    flex: 1;
    min-width: 0;
    font-size: 0.9rem;
    color: var(--fg-dim);
  }

  .new {
    gap: 4px;
  }

  .grow {
    flex: 1;
  }

  .delete {
    align-self: flex-start;
    gap: 6px;
    margin-top: 2px;
  }

  .what.sel {
    color: var(--accent);
    font-weight: 500;
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
