<script lang="ts">
  // Editor for one effect parameter, chosen by its kind:
  //   int / float  slider (log scale for wide ranges) + exact number
  //   choice       dropdown
  //   size         text (rows, px, % or bytes) + log slider for rows and %
  //   region       one two-handle slider while both ends are percents, + text;
  //                dragging draws the band on the preview
  //   curve        a curve editor (CurveField) + text
  // Sliders update the preview live and record one history entry on release.
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import { app } from '../lib/app.svelte';
  import { SLIDER_STEPS, percentScale, rowsScale, sliderScale } from '../lib/slider';
  import type { ParamInfo } from '../lib/types';
  import DualRange from './DualRange.svelte';
  import CurveField from './CurveField.svelte';

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
  {:else if spec.kind === 'curve'}
    <CurveField {uid} {text} oninput={live} onchange={(v) => set(v)} />
    <input
      {id}
      class="num wide"
      type="text"
      value={text}
      onchange={(e) => commitField(e.currentTarget)}
      spellcheck="false"
      autocomplete="off"
      aria-label="{spec.name} as text"
    />
  {:else if spec.kind === 'region'}
    <div class="ctl">
      {#if region}
        {@const r = region}
        <DualRange
          lo={r.s}
          hi={r.e}
          labelLo="region start (percent from the top)"
          labelHi="region end (percent from the top)"
          oninput={dragRegion}
          onchange={endRegion}
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

  /* A curve's text form, for exact or pasted values. */
  .num.wide[type='text'] {
    width: 100%;
    text-align: left;
    font-size: 0.8rem;
  }

  .num[type='text'] {
    width: 10ch;
  }

  @media (pointer: coarse) {
    .reset {
      min-height: 32px;
      min-width: 32px;
    }
  }
</style>
