<script lang="ts">
  // Editor for one effect parameter, chosen by its kind:
  //   int / float  slider (log scale for wide ranges) + exact number
  //   choice       dropdown
  //   size         text (rows, px, % or bytes) + log slider for rows and %
  //   region       one two-handle slider while both ends are percents, + text;
  //                dragging draws the band on the preview
  // Sliders update the preview live and record one history entry on release.
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import { app } from '../lib/app.svelte';
  import { SLIDER_STEPS, percentScale, rowsScale, sliderScale } from '../lib/slider';
  import type { ParamInfo } from '../lib/types';

  let { uid, spec, value }: { uid: string; spec: ParamInfo; value: string | undefined } = $props();

  const text = $derived(value ?? spec.default);
  const overridden = $derived(value !== undefined);
  const id = $derived(`${uid}-${spec.name}`);

  const numeric = $derived(spec.kind === 'int' || spec.kind === 'float');
  const scale = $derived(numeric ? sliderScale(spec.min ?? 0, spec.max ?? 1, spec.kind === 'int') : null);

  /** Live update while dragging; history is recorded on `change`. */
  const live = (v: string) => app.setParam(uid, spec.name, v, false);
  const set = (v: string | null) => app.setParam(uid, spec.name, v, true);

  /**
   * Apply a typed value, then show the canonical one. Without the reset, a
   * field could keep showing text that was rejected or normalized (an
   * invalid number reads as "" and falls back to the default), because the
   * bound value itself did not change.
   */
  function commitField(el: HTMLInputElement) {
    if (el.type === 'number' && el.value === '' && el.validity.badInput) {
      el.value = text; // not a number: keep the current value
      return;
    }
    set(el.value);
    el.value = text;
  }

  // Size: a value in rows or percent gets a log slider in that unit.
  const pct = (s: string) => {
    const m = /^\s*([\d.]+)\s*%\s*$/.exec(s);
    return m ? Number(m[1]) : null;
  };
  const rows = (s: string) => {
    const m = /^\s*([\d.]+)\s*rows?\s*$/.exec(s);
    return m ? Number(m[1]) : null;
  };
  const sizeSlider = $derived.by(() => {
    if (spec.kind !== 'size') return null;
    const p = pct(text);
    if (p !== null) return { scale: percentScale, value: p, unit: '%' };
    const r = rows(text);
    if (r !== null) return { scale: rowsScale, value: r, unit: 'rows' };
    return null;
  });

  // Region: START:END with both ends in percent gets two sliders.
  const region = $derived.by(() => {
    if (spec.kind !== 'region') return null;
    const [a, b] = text.split(':');
    const s = pct(a ?? '');
    const e = pct(b ?? '');
    return s === null || e === null ? null : { s, e };
  });

  /** Live region change from a handle; the preview draws the band. */
  function dragRegion(s: number, e: number) {
    app.showRegion(s, e);
    live(`${s}%:${e}%`);
  }

  function endRegion() {
    app.commit();
    app.hideRegion();
  }
</script>

<div class="field">
  <div class="top">
    <label for={id} title={spec.help}>{spec.name}</label>
    <span class="help dim" title={spec.help}>{spec.help}</span>
    {#if overridden}
      <button class="icon reset" onclick={() => set(null)} title="Back to the default ({spec.default})" aria-label="Reset {spec.name}">
        <RotateCcw size={14} strokeWidth={2} />
      </button>
    {/if}
  </div>

  {#if numeric && scale}
    <div class="ctl">
      <input
        type="range"
        min="0"
        max={SLIDER_STEPS}
        step="1"
        value={scale.toSlider(Number(text))}
        style:--pct="{(scale.toSlider(Number(text)) / SLIDER_STEPS) * 100}%"
        oninput={(e) => live(String(scale.fromSlider(Number(e.currentTarget.value))))}
        onchange={() => app.commit()}
        aria-label="{spec.name} slider"
      />
      <input
        {id}
        class="num"
        type="number"
        value={text}
        min={spec.min}
        max={spec.max}
        step={spec.kind === 'int' ? 1 : 0.01}
        onchange={(e) => commitField(e.currentTarget)}
      />
    </div>
  {:else if spec.kind === 'choice'}
    <select {id} value={text} onchange={(e) => set(e.currentTarget.value)}>
      {#each spec.choices ?? [] as c (c)}<option value={c}>{c}</option>{/each}
    </select>
  {:else if spec.kind === 'size'}
    <div class="ctl">
      {#if sizeSlider}
        {@const sl = sizeSlider}
        <input
          type="range"
          min="0"
          max={SLIDER_STEPS}
          step="1"
          value={sl.scale.toSlider(sl.value)}
          style:--pct="{(sl.scale.toSlider(sl.value) / SLIDER_STEPS) * 100}%"
          oninput={(e) => live(`${sl.scale.fromSlider(Number(e.currentTarget.value))}${sl.unit}`)}
          onchange={() => app.commit()}
          aria-label="{spec.name} slider"
        />
      {/if}
      <input
        {id}
        class="num"
        type="text"
        value={text}
        onchange={(e) => commitField(e.currentTarget)}
        spellcheck="false"
        autocomplete="off"
      />
    </div>
  {:else if spec.kind === 'region'}
    <div class="ctl">
      {#if region}
        {@const r = region}
        <!-- Two range inputs stacked on one track. Only their thumbs take
             pointer events, so each handle can be grabbed where it is.
             The handle nearer the free side sits on top, so two handles
             pushed together can always be pulled apart again. -->
        <div class="dual" class:low-top={r.s > 50}>
          <div class="fill" style:left="{r.s}%" style:width="{r.e - r.s}%"></div>
          <input
            class="lo"
            type="range"
            min="0"
            max="100"
            step="0.5"
            value={r.s}
            oninput={(e) => {
              const v = Math.min(Number(e.currentTarget.value), r.e);
              e.currentTarget.value = String(v);
              dragRegion(v, r.e);
            }}
            onchange={endRegion}
            aria-label="region start (percent from the top)"
          />
          <input
            class="hi"
            type="range"
            min="0"
            max="100"
            step="0.5"
            value={r.e}
            oninput={(e) => {
              const v = Math.max(Number(e.currentTarget.value), r.s);
              e.currentTarget.value = String(v);
              dragRegion(r.s, v);
            }}
            onchange={endRegion}
            aria-label="region end (percent from the top)"
          />
        </div>
      {/if}
      <input
        {id}
        class="num"
        type="text"
        value={text}
        onchange={(e) => commitField(e.currentTarget)}
        spellcheck="false"
        autocomplete="off"
      />
    </div>
  {/if}
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .top {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
  }

  label {
    font-size: 0.9rem;
    font-weight: 500;
    text-transform: capitalize;
  }

  .help {
    flex: 1;
    min-width: 0;
    font-size: 0.75rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .reset {
    min-height: 24px;
    min-width: 24px;
    padding: 3px;
    align-self: center;
  }

  .ctl {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .num {
    width: 8ch;
    flex: none;
    text-align: right;
  }

  .num[type='text'] {
    width: 10ch;
  }

  /* Two-handle region slider: two range inputs stacked on one track. */
  .dual {
    position: relative;
    flex: 1;
    min-width: 0;
    height: 28px;
  }

  /* The track, drawn once for both inputs. */
  .dual::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    height: 4px;
    border-radius: 2px;
    transform: translateY(-50%);
    background: var(--fill-strong);
  }

  .fill {
    position: absolute;
    top: 50%;
    height: 4px;
    transform: translateY(-50%);
    background: var(--accent);
  }

  .dual input {
    position: absolute;
    inset: 0;
    height: 100%;
    min-height: 0;
    pointer-events: none;
  }

  .dual .hi {
    z-index: 2;
  }

  .dual.low-top .lo {
    z-index: 3;
  }

  .dual input::-webkit-slider-runnable-track {
    background: none;
  }

  .dual input::-webkit-slider-thumb {
    pointer-events: auto;
    cursor: grab;
  }

  .dual input::-moz-range-track,
  .dual input::-moz-range-progress {
    background: none;
  }

  .dual input::-moz-range-thumb {
    pointer-events: auto;
    cursor: grab;
  }

  @media (pointer: coarse) {
    .reset {
      min-height: 32px;
      min-width: 32px;
    }
    .dual {
      height: 44px;
    }
  }
</style>
