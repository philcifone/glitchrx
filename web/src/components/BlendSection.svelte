<script lang="ts">
  // A step's blend settings: opacity, and the mask that decides where the
  // step's result shows. Painting happens on the preview (MaskPainter);
  // this section holds the tools and the list of mask operations.
  import Paintbrush from '@lucide/svelte/icons/paintbrush';
  import Eraser from '@lucide/svelte/icons/eraser';
  import Blinds from '@lucide/svelte/icons/blinds';
  import CircleDot from '@lucide/svelte/icons/circle-dot';
  import SunMedium from '@lucide/svelte/icons/sun-medium';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../lib/app.svelte';
  import { SLIDER_STEPS } from '../lib/slider';
  import { brushScale, featherScale, opRows, percent, type MaskTool } from '../lib/mask';
  import type { MaskMode, StepState } from '../lib/types';
  import DualRange from './DualRange.svelte';

  let { step }: { step: StepState } = $props();

  const uid = $derived(step.uid);
  const mask = $derived(step.mask);
  const painting = $derived(app.masking === step.uid);
  const opacity = $derived(app.opacity(step));
  const rows = $derived(opRows(mask?.op ?? []));
  const feather = $derived(mask?.feather ?? 0);
  const brushy = $derived(app.tool.tool === 'brush' || app.tool.tool === 'eraser');

  const TOOLS: { id: MaskTool; label: string; icon: typeof Paintbrush; key: string }[] = [
    { id: 'brush', label: 'Brush', icon: Paintbrush, key: 'B' },
    { id: 'eraser', label: 'Eraser', icon: Eraser, key: 'E' },
    { id: 'linear', label: 'Linear', icon: Blinds, key: 'G' },
    { id: 'radial', label: 'Radial', icon: CircleDot, key: 'R' },
  ];
  const MODES: { id: MaskMode; label: string }[] = [
    { id: 'add', label: 'Add' },
    { id: 'subtract', label: 'Subtract' },
    { id: 'intersect', label: 'Intersect' },
  ];

  /** A brightness range: a limit (intersect) once something is painted. */
  function addBrightness() {
    const painted = (mask?.op ?? []).length > 0;
    app.addMaskOp(uid, { shape: 'brightness', mode: painted ? 'intersect' : 'add', low: 0.5, high: 1, soft: 0.1 });
  }

  // Bring the tools into view when painting starts (the phone sheet is short).
  let section = $state<HTMLElement>();
  $effect(() => {
    if (painting) section?.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  });

  const sliderPct = (t: number) => `${(t / SLIDER_STEPS) * 100}%`;
</script>

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
      <span class="grow dim note">None: shows everywhere</span>
      <button onclick={() => app.addMask(uid)} disabled={!step.enabled}>Add mask</button>
    </div>
  {:else}
    <div class="row">
      <span class="lbl">Mask</span>
      <span class="grow"></span>
      {#if painting}
        <button class="primary" onclick={() => app.stopMasking()}>Done</button>
      {:else}
        <button onclick={() => app.startMasking(uid)} disabled={!step.enabled || !app.doc}>Paint</button>
      {/if}
      <button class="icon danger" onclick={() => app.removeMask(uid)} title="Remove the mask" aria-label="Remove the mask">
        <Trash2 size={18} strokeWidth={1.75} />
      </button>
    </div>

    <div class="segmented" role="radiogroup" aria-label="What painting does">
      <button role="radio" aria-checked={!mask.invert} class:on={!mask.invert} onclick={() => app.updateMask(uid, { invert: false })}>
        Paint in
      </button>
      <button role="radio" aria-checked={!!mask.invert} class:on={!!mask.invert} onclick={() => app.updateMask(uid, { invert: true })}>
        Paint out
      </button>
    </div>
    <p class="note dim">
      {mask.invert ? 'The glitch shows everywhere except where you paint.' : 'The glitch shows only where you paint.'}
    </p>

    {#if painting}
      <div class="segmented tools" role="radiogroup" aria-label="Tool">
        {#each TOOLS as t (t.id)}
          {@const Icon = t.icon}
          <button
            role="radio"
            aria-checked={app.tool.tool === t.id}
            class:on={app.tool.tool === t.id}
            onclick={() => app.setTool({ tool: t.id })}
            title="{t.label} ({t.key})"
          >
            <Icon size={16} strokeWidth={1.75} />
            <span>{t.label}</span>
          </button>
        {/each}
      </div>
      {#if brushy}
        <div class="row">
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
        <div class="row">
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
      {:else}
        <p class="note dim">
          Drag on the photo: {app.tool.tool === 'linear'
            ? 'full where you start, fading out where you let go.'
            : 'full at the centre, fading out to the edge.'}
        </p>
      {/if}
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

    {#if rows.length}
      <ul class="ops">
        {#each rows as row (row.kind === 'strokes' ? 'strokes' : row.index)}
          <li>
            {#if row.kind === 'strokes'}
              <div class="row">
                <span class="glyph"><Paintbrush size={16} strokeWidth={1.75} /></span>
                <span class="grow">
                  {row.painted} {row.painted === 1 ? 'stroke' : 'strokes'}{row.erased ? `, ${row.erased} erased` : ''}
                </span>
                <button class="icon" onclick={() => app.removeMaskOps(uid, row.indices)} title="Remove all strokes" aria-label="Remove all strokes">
                  <X size={16} strokeWidth={1.75} />
                </button>
              </div>
            {:else}
              {@const op = row.op}
              <div class="row">
                <span class="glyph">
                  {#if op.shape === 'linear'}<Blinds size={16} strokeWidth={1.75} />
                  {:else if op.shape === 'radial'}<CircleDot size={16} strokeWidth={1.75} />
                  {:else}<SunMedium size={16} strokeWidth={1.75} />{/if}
                </span>
                <span class="grow name">{op.shape === 'brightness' ? 'Brightness' : op.shape === 'linear' ? 'Linear gradient' : 'Radial gradient'}</span>
                <select value={op.mode} onchange={(e) => app.setMaskOpMode(uid, row.index, e.currentTarget.value as MaskMode)} aria-label="How it combines">
                  {#each MODES as m (m.id)}<option value={m.id}>{m.label}</option>{/each}
                </select>
                <button class="icon" onclick={() => app.removeMaskOps(uid, [row.index])} title="Remove" aria-label="Remove">
                  <X size={16} strokeWidth={1.75} />
                </button>
              </div>
              {#if op.shape === 'brightness'}
                <div class="row sub">
                  <span class="lbl">Range</span>
                  <DualRange
                    lo={Math.round(op.low * 100)}
                    hi={Math.round(op.high * 100)}
                    step={1}
                    labelLo="Darkest brightness included"
                    labelHi="Brightest brightness included"
                    oninput={(lo, hi) => app.updateMaskOp(uid, row.index, { low: lo / 100, high: hi / 100 }, false)}
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
                    oninput={(e) => app.updateMaskOp(uid, row.index, { soft: Number(e.currentTarget.value) / 100 }, false)}
                    onchange={() => app.commit()}
                    aria-label="Softness of the brightness range"
                  />
                  <span class="val">{Math.round(op.soft * 100)}</span>
                </div>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <button class="plain add" onclick={addBrightness}>
      <SunMedium size={16} strokeWidth={1.75} />
      Add brightness range
    </button>
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

  .tools button {
    padding: 4px 6px;
    gap: 4px;
    font-size: 0.82rem;
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
    gap: 6px;
    padding: 6px 8px;
    background: var(--fill);
    border-radius: var(--radius);
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
    width: 3.5em;
    font-weight: 400;
    font-size: 0.85rem;
  }

  .add {
    align-self: flex-start;
    color: var(--accent);
  }

  @media (max-width: 380px) {
    .tools span {
      display: none;
    }
  }
</style>
