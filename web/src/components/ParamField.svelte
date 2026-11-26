<script lang="ts">
  // Editor for one effect parameter, chosen by its kind:
  //   int / float  slider (log scale for wide ranges) + exact number
  //   choice       a segmented control when the options fit on one line,
  //                a dropdown otherwise (Phil wants to see the options)
  //   set          a row of toggles (color dots for color ranges)
  //   size         text (rows, px, % or bytes) + log slider for rows, px and %
  //   region       one two-handle slider while both ends are percents, + text;
  //                dragging draws the band on the preview
  //   curve        a curve editor (CurveField) + text
  // Sliders update the preview live and record one history entry on release.
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import { app } from '../lib/app.svelte';
  import { percentScale, pxScale, rowsScale, sliderScale } from '../lib/slider';
  import type { ParamInfo } from '../lib/types';
  import DualRange from './DualRange.svelte';
  import RangeInput from './RangeInput.svelte';
  import CurveField from './CurveField.svelte';
  import SetField from './SetField.svelte';

  let { uid, spec, value }: { uid: string; spec: ParamInfo; value: string | undefined } = $props();

  const text = $derived(value ?? spec.default);
  const overridden = $derived(value !== undefined);
  const id = $derived(`${uid}-${spec.name}`);

  /** The help line shows in full after a tap; one line (cut short) otherwise. */
  let helpOpen = $state(false);

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
  const pxs = (s: string) => {
    const m = /^\s*(\d+)\s*px\s*$/.exec(s);
    return m ? Number(m[1]) : null;
  };
  const rows = (s: string) => {
    const m = /^\s*([\d.]+)\s*rows?\s*$/.exec(s);
    return m ? Number(m[1]) : null;
  };
  const sizeSlider = $derived.by(() => {
    if (spec.kind !== 'size') return null;
    const p = pct(text);
    // The default gets a tick only when it is in the same unit.
    if (p !== null) {
      const d = pct(spec.default);
      return { scale: percentScale, value: p, unit: '%', dflt: d === null ? null : Math.round(percentScale.toSlider(d)) };
    }
    const x = pxs(text);
    if (x !== null) {
      const d = pxs(spec.default);
      return { scale: pxScale, value: x, unit: 'px', dflt: d === null ? null : Math.round(pxScale.toSlider(d)) };
    }
    const r = rows(text);
    if (r !== null) {
      const d = rows(spec.default);
      return { scale: rowsScale, value: r, unit: 'rows', dflt: d === null ? null : Math.round(rowsScale.toSlider(d)) };
    }
    return null;
  });

  /**
   * Choices as a segmented control when they fit a phone's card: a few
   * short words. Longer lists keep the dropdown.
   */
  const segmented = $derived.by(() => {
    const c = spec.choices ?? [];
    return spec.kind === 'choice' && c.length <= 5 && c.join('').length <= 28;
  });

  /** Whether the field shows a slider (which resets with a double tap). */
  const hasSlider = $derived((numeric && scale !== null) || sizeSlider !== null);

  /** Live value from a slider position; the default exactly when on its tick. */
  function slide(p: number, dflt: number | null, value: () => string) {
    live(p === dflt ? spec.default : value());
  }

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
    <!-- Cut to one line to keep the card short; a tap shows all of it
         (phones have no hover for the title). -->
    <button
      type="button"
      class="help dim"
      class:open={helpOpen}
      onclick={() => (helpOpen = !helpOpen)}
      aria-expanded={helpOpen}
      title={spec.help}
    >
      {spec.help}
    </button>
    <!-- Sliders reset with a double tap (D-55), so only fields without
         one get a reset button. It stays when at the default (dimmed), so
         the row never changes height. -->
    {#if !hasSlider}
      <button
        class="icon reset"
        onclick={() => set(null)}
        disabled={!overridden}
        title="Back to the default ({spec.default})"
        aria-label="Reset {spec.name}"
      >
        <RotateCcw size={14} strokeWidth={2} />
      </button>
    {/if}
  </div>

  {#if numeric && scale}
    {@const dflt = Math.round(scale.toSlider(Number(spec.default)))}
    <div class="ctl">
      <RangeInput
        pos={scale.toSlider(Number(text))}
        defaultPos={dflt}
        label="{spec.name} slider"
        oninput={(p) => slide(p, dflt, () => String(scale.fromSlider(p)))}
        onchange={() => app.commit()}
        onreset={() => set(null)}
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
  {:else if spec.kind === 'choice' && segmented}
    <div class="segmented choice" role="radiogroup" aria-label={spec.name}>
      {#each spec.choices ?? [] as c (c)}
        <button role="radio" aria-checked={text === c} class:on={text === c} onclick={() => set(c === spec.default ? null : c)}>{c}</button>
      {/each}
    </div>
  {:else if spec.kind === 'choice'}
    <select {id} value={text} onchange={(e) => set(e.currentTarget.value)}>
      {#each spec.choices ?? [] as c (c)}<option value={c}>{c}</option>{/each}
    </select>
  {:else if spec.kind === 'set'}
    <SetField {id} options={spec.choices ?? []} {text} onchange={(v) => set(v)} />
  {:else if spec.kind === 'size'}
    <div class="ctl">
      {#if sizeSlider}
        {@const sl = sizeSlider}
        <RangeInput
          pos={sl.scale.toSlider(sl.value)}
          defaultPos={sl.dflt}
          label="{spec.name} slider"
          oninput={(p) => slide(p, sl.dflt, () => `${sl.scale.fromSlider(p)}${sl.unit}`)}
          onchange={() => app.commit()}
          onreset={() => set(null)}
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
    /* Plain text, not a button look; a block, since the ellipsis does not
       work in the flex box buttons get by default. */
    display: block;
    min-height: 0;
    appearance: none;
    background: none;
    border: 0;
    border-radius: 0;
    padding: 0;
    margin: 0;
    font-weight: 400;
    font: inherit;
    font-size: 0.75rem;
    color: var(--fg-dim);
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .help:hover {
    background: none;
  }

  .help.open {
    white-space: normal;
    overflow: visible;
  }

  .reset:disabled {
    opacity: 0.3;
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

  /* Every option in view, the card's width. */
  .choice {
    display: flex;
    width: 100%;
  }

  .choice button {
    flex: 1;
    min-width: 0;
    padding-left: 2px;
    padding-right: 2px;
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
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
