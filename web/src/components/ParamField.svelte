<script lang="ts">
  // Editor for one effect parameter, chosen by its kind:
  //   int / float  slider (log scale for wide ranges) + exact number
  //   choice       dropdown
  //   size         text (bytes, 4k, 1m or %) + log slider while it is a percent
  //   region       start/end sliders while both ends are percents, + text
  // Sliders update the preview live and record one history entry on release.
  import RotateCcw from '@lucide/svelte/icons/rotate-ccw';
  import { app } from '../lib/app.svelte';
  import { SLIDER_STEPS, percentScale, sliderScale } from '../lib/slider';
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

  // Size: a percent value gets a slider.
  const pct = (s: string) => {
    const m = /^\s*([\d.]+)\s*%\s*$/.exec(s);
    return m ? Number(m[1]) : null;
  };
  const sizePct = $derived(spec.kind === 'size' ? pct(text) : null);

  // Region: START:END with both ends in percent gets two sliders.
  const region = $derived.by(() => {
    if (spec.kind !== 'region') return null;
    const [a, b] = text.split(':');
    const s = pct(a ?? '');
    const e = pct(b ?? '');
    return s === null || e === null ? null : { s, e };
  });

  function setRegion(s: number, e: number, commit: boolean) {
    const v = `${Math.min(s, e)}%:${Math.max(s, e)}%`;
    if (commit) set(v);
    else live(v);
  }
</script>

<div class="field">
  <div class="top">
    <label for={id} title={spec.help}>{spec.name}</label>
    <span class="help dim" title={spec.help}>{spec.help}</span>
    {#if overridden}
      <button class="icon reset" onclick={() => set(null)} title="Back to the default ({spec.default})" aria-label="Reset {spec.name}">
        <RotateCcw size={14} strokeWidth={2.5} />
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
      {#if sizePct !== null}
        <input
          type="range"
          min="0"
          max={SLIDER_STEPS}
          step="1"
          value={percentScale.toSlider(sizePct)}
          oninput={(e) => live(`${percentScale.fromSlider(Number(e.currentTarget.value))}%`)}
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
    {#if region}
      <div class="ctl">
        <span class="dim lab">from</span>
        <input
          type="range"
          min="0"
          max="100"
          step="0.5"
          value={region.s}
          oninput={(e) => setRegion(Number(e.currentTarget.value), region.e, false)}
          onchange={() => app.commit()}
          aria-label="region start"
        />
      </div>
      <div class="ctl">
        <span class="dim lab">to</span>
        <input
          type="range"
          min="0"
          max="100"
          step="0.5"
          value={region.e}
          oninput={(e) => setRegion(region.s, Number(e.currentTarget.value), false)}
          onchange={() => app.commit()}
          aria-label="region end"
        />
      </div>
    {/if}
    <input
      {id}
      type="text"
      value={text}
      onchange={(e) => commitField(e.currentTarget)}
      spellcheck="false"
      autocomplete="off"
    />
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
    font-weight: 700;
  }

  .help {
    flex: 1;
    min-width: 0;
    font-size: 0.8em;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .reset {
    min-height: 24px;
    min-width: 24px;
    padding: 2px;
    border-width: 1.5px;
    align-self: center;
  }

  .ctl {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .num {
    width: 9ch;
    flex: none;
  }

  .lab {
    width: 4ch;
    flex: none;
    font-size: 0.85em;
  }

  @media (pointer: coarse) {
    .reset {
      min-height: 32px;
      min-width: 32px;
    }
  }
</style>
