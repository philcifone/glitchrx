<script lang="ts">
  // A step's blend settings: opacity, and the mask that decides where the
  // step's result shows. The mask is a list of components; tapping one
  // makes it active, and its handles (or its brush) work on the preview
  // (MaskPainter). The + row adds another; removing the last one removes
  // the mask, so the step shows everywhere again.
  import Paintbrush from '@lucide/svelte/icons/paintbrush';
  import Eraser from '@lucide/svelte/icons/eraser';
  import Blinds from '@lucide/svelte/icons/blinds';
  import CircleDot from '@lucide/svelte/icons/circle-dot';
  import SunMedium from '@lucide/svelte/icons/sun-medium';
  import Plus from '@lucide/svelte/icons/plus';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../lib/app.svelte';
  import { SLIDER_STEPS } from '../lib/slider';
  import { SHAPE_LABEL, brushScale, featherScale, opDetail, percent } from '../lib/mask';
  import type { MaskMode, MaskShape, StepState } from '../lib/types';
  import DualRange from './DualRange.svelte';

  let { step }: { step: StepState } = $props();

  const uid = $derived(step.uid);
  const mask = $derived(step.mask);
  const ops = $derived(mask?.op ?? []);
  const editing = $derived(app.masking === step.uid);
  const opacity = $derived(app.opacity(step));
  const feather = $derived(mask?.feather ?? 0);
  const usable = $derived(step.enabled && !!app.doc);

  const ICON: Record<MaskShape, typeof Paintbrush> = {
    brush: Paintbrush,
    linear: Blinds,
    radial: CircleDot,
    brightness: SunMedium,
  };
  const SHAPES: MaskShape[] = ['brush', 'linear', 'radial', 'brightness'];
  const MODES: { id: MaskMode; label: string }[] = [
    { id: 'add', label: 'Add' },
    { id: 'subtract', label: 'Subtract' },
    { id: 'intersect', label: 'Intersect' },
  ];
  const HINT: Record<MaskShape, string> = {
    brush: 'Paint on the photo. Erase takes back only this brush.',
    linear: 'Full at the solid line, faded out at the other. Drag a line or the middle; drag elsewhere to draw it again.',
    radial: 'Drag the centre to move it and the ring to resize it; drag elsewhere to draw it again. Feather softens its edge.',
    brightness: 'Pixels in this range, read from the picture as it reaches this step.',
  };

  /** The + menu under the list. */
  let adding = $state(false);
  function add(shape: MaskShape) {
    adding = false;
    app.addComponent(uid, shape);
  }

  const isActive = (i: number) => editing && app.activeOp === i;

  // Bring the controls into view when editing starts (the phone sheet is short).
  let section = $state<HTMLElement>();
  $effect(() => {
    if (editing) section?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  });

  const sliderPct = (t: number) => `${(t / SLIDER_STEPS) * 100}%`;
</script>

{#snippet addButtons()}
  <div class="adds" role="group" aria-label="Add to the mask">
    {#each SHAPES as shape (shape)}
      {@const Icon = ICON[shape]}
      <button onclick={() => add(shape)} disabled={!usable}>
        <Icon size={16} strokeWidth={1.75} />
        <span>{SHAPE_LABEL[shape]}</span>
      </button>
    {/each}
  </div>
{/snippet}

<section class="group" aria-label="Blend" bind:this={section}>
  <h3 class="section-label">Blend</h3>

  <div class="row">
    <span class="lbl">Opacity</span>
    <input
      type="range"
      min="0"
      max="100"
      step="1"
      value={Math.round(opacity * 100)}
      style:--pct="{opacity * 100}%"
      oninput={(e) => app.setOpacity(uid, Number(e.currentTarget.value) / 100, false)}
      onchange={() => app.commit()}
      aria-label="Opacity"
    />
    <span class="val">{Math.round(opacity * 100)}%</span>
  </div>

  {#if !mask}
    <div class="row">
      <span class="lbl">Mask</span>
      <span class="grow dim note">None: shows everywhere. Start one with:</span>
    </div>
    {@render addButtons()}
  {:else}
    <div class="row">
      <span class="lbl">Mask</span>
      <span class="grow"></span>
      {#if editing}
        <button class="primary" onclick={() => app.stopMasking()}>Done</button>
      {:else}
        <button onclick={() => app.startMasking(uid)} disabled={!usable}>Edit</button>
      {/if}
    </div>

    <div class="segmented" role="radiogroup" aria-label="Where the glitch shows">
      <button role="radio" aria-checked={!mask.invert} class:on={!mask.invert} onclick={() => app.updateMask(uid, { invert: false })}>
        Paint in
      </button>
      <button role="radio" aria-checked={!!mask.invert} class:on={!!mask.invert} onclick={() => app.updateMask(uid, { invert: true })}>
        Paint out
      </button>
    </div>
    <p class="note dim">
      {mask.invert ? 'The glitch shows everywhere except the mask.' : 'The glitch shows only inside the mask.'}
    </p>

    <ul class="ops" aria-label="Mask components">
      {#each ops as op, i (i)}
        {@const Icon = ICON[op.shape]}
        {@const on = isActive(i)}
        <li class:active={on}>
          <div class="row">
            <button
              class="pick"
              aria-pressed={on}
              onclick={() => app.selectComponent(uid, i)}
              disabled={!usable}
              title={on ? 'Being edited' : 'Edit this part of the mask'}
            >
              <span class="glyph"><Icon size={16} strokeWidth={1.75} /></span>
              <span class="text">
                <span class="name">{SHAPE_LABEL[op.shape]}</span>
                {#if opDetail(op)}<span class="detail dim">{opDetail(op)}</span>{/if}
              </span>
            </button>
            <select value={op.mode} onchange={(e) => app.setMaskOpMode(uid, i, e.currentTarget.value as MaskMode)} aria-label="How it combines">
              {#each MODES as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
            </select>
            <button class="icon" onclick={() => app.removeMaskOps(uid, [i])} title="Remove" aria-label="Remove {SHAPE_LABEL[op.shape]}">
              <X size={16} strokeWidth={1.75} />
            </button>
          </div>

          {#if on}
            {#if op.shape === 'brush'}
              <div class="segmented" role="radiogroup" aria-label="Brush">
                <button role="radio" aria-checked={app.tool.tool === 'paint'} class:on={app.tool.tool === 'paint'} onclick={() => app.setTool({ tool: 'paint' })} title="Paint (B)">
                  <Paintbrush size={15} strokeWidth={1.75} /> Paint
                </button>
                <button role="radio" aria-checked={app.tool.tool === 'erase'} class:on={app.tool.tool === 'erase'} onclick={() => app.setTool({ tool: 'erase' })} title="Erase (E)">
                  <Eraser size={15} strokeWidth={1.75} /> Erase
                </button>
              </div>
              <div class="row sub">
                <span class="lbl">Size</span>
                <input
                  type="range"
                  min="0"
                  max={SLIDER_STEPS}
                  step="1"
                  value={brushScale.toSlider(app.tool.size)}
                  style:--pct={sliderPct(brushScale.toSlider(app.tool.size))}
                  oninput={(e) => app.setTool({ size: brushScale.fromSlider(Number(e.currentTarget.value)) })}
                  aria-label="Brush size"
                  title="Brush size ([ and ] keys)"
                />
                <span class="val">{percent(app.tool.size)}</span>
              </div>
              <div class="row sub">
                <span class="lbl">Hardness</span>
                <input
                  type="range"
                  min="0"
                  max="100"
                  step="1"
                  value={Math.round(app.tool.hardness * 100)}
                  style:--pct="{app.tool.hardness * 100}%"
                  oninput={(e) => app.setTool({ hardness: Number(e.currentTarget.value) / 100 })}
                  aria-label="Brush hardness"
                />
                <span class="val">{Math.round(app.tool.hardness * 100)}%</span>
              </div>
            {:else if op.shape === 'brightness'}
              <div class="row sub">
                <span class="lbl">Range</span>
                <DualRange
                  lo={Math.round(op.low * 100)}
                  hi={Math.round(op.high * 100)}
                  step={1}
                  labelLo="Darkest brightness included"
                  labelHi="Brightest brightness included"
                  oninput={(lo, hi) => app.updateMaskOp(uid, i, { low: lo / 100, high: hi / 100 }, false)}
                  onchange={() => app.commit()}
                />
                <span class="val">{Math.round(op.low * 100)}-{Math.round(op.high * 100)}</span>
              </div>
              <div class="row sub">
                <span class="lbl">Soft</span>
                <input
                  type="range"
                  min="0"
                  max="50"
                  step="1"
                  value={Math.round(op.soft * 100)}
                  style:--pct="{op.soft * 200}%"
                  oninput={(e) => app.updateMaskOp(uid, i, { soft: Number(e.currentTarget.value) / 100 }, false)}
                  onchange={() => app.commit()}
                  aria-label="Softness of the brightness range"
                />
                <span class="val">{Math.round(op.soft * 100)}</span>
              </div>
            {/if}
            <p class="note dim">{HINT[op.shape]}</p>
          {/if}
        </li>
      {/each}
    </ul>

    {#if adding || ops.length === 0}
      {@render addButtons()}
    {:else}
      <button class="plain add" onclick={() => (adding = true)} disabled={!usable}>
        <Plus size={16} strokeWidth={1.75} />
        Add to mask
      </button>
    {/if}

    {#if editing}
      <label class="row">
        <span class="lbl wide">Show mask</span>
        <span class="grow"></span>
        <input class="switch" type="checkbox" checked={app.tool.show} onchange={(e) => app.setTool({ show: e.currentTarget.checked })} />
      </label>
    {/if}

    <div class="row">
      <span class="lbl">Feather</span>
      <input
        type="range"
        min="0"
        max={SLIDER_STEPS}
        step="1"
        value={featherScale.toSlider(feather)}
        style:--pct={sliderPct(featherScale.toSlider(feather))}
        oninput={(e) => app.updateMask(uid, { feather: featherScale.fromSlider(Number(e.currentTarget.value)) }, false)}
        onchange={() => app.commit()}
        aria-label="Feather"
        title="Softens every edge of the mask"
      />
      <span class="val">{percent(feather)}</span>
    </div>
  {/if}
</section>

<style>
  .group {
    border-top: var(--hairline);
    padding: 12px 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
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

  .grow {
    flex: 1;
    min-width: 0;
  }

  .note {
    margin: 0;
    font-size: 0.8rem;
  }

  .adds {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }

  .adds button {
    flex-direction: column;
    gap: 2px;
    padding: 6px 4px;
    font-size: 0.78rem;
    color: var(--accent);
  }

  .ops {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .ops li {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 6px 8px;
    background: var(--fill);
    border-radius: var(--radius);
    /* The outline is drawn inside, so selecting a row does not move it. */
    box-shadow: inset 0 0 0 1.5px transparent;
    transition: box-shadow 0.15s;
  }

  .ops li.active {
    box-shadow: inset 0 0 0 1.5px var(--accent);
    padding-bottom: 10px;
  }

  /* The row's name area: tap to make the component active. */
  .pick {
    flex: 1;
    min-width: 0;
    justify-content: flex-start;
    gap: 8px;
    padding: 4px 2px;
    background: none;
    border: none;
    color: inherit;
    text-align: left;
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
  }

  .detail {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.8rem;
  }

  .ops .segmented button {
    gap: 6px;
  }

  .ops .row {
    gap: 8px;
  }

  .ops select {
    width: auto;
    min-height: 30px;
    font-size: 0.85rem;
  }

  .ops .icon {
    min-height: 30px;
    min-width: 30px;
    padding: 4px;
  }

  .name {
    font-size: 0.9rem;
  }

  .lbl.wide {
    width: auto;
  }

  .glyph {
    display: inline-flex;
    color: var(--accent);
  }

  .sub .lbl {
    width: 4.8em;
    font-weight: 400;
    font-size: 0.85rem;
  }

  .add {
    align-self: flex-start;
    color: var(--accent);
  }
</style>
