<script lang="ts">
  // A two-handle range slider: two range inputs stacked on one track. Only
  // their thumbs take pointer events, so each handle can be grabbed where
  // it is. The handle nearer the free side sits on top, so two handles
  // pushed together can always be pulled apart again.
  let {
    lo,
    hi,
    min = 0,
    max = 100,
    step = 0.5,
    labelLo,
    labelHi,
    oninput,
    onchange,
  }: {
    lo: number;
    hi: number;
    min?: number;
    max?: number;
    step?: number;
    labelLo: string;
    labelHi: string;
    /** Live change from either handle. */
    oninput: (lo: number, hi: number) => void;
    /** A handle was released. */
    onchange: () => void;
  } = $props();

  const at = (v: number) => ((v - min) / (max - min)) * 100;
</script>

<div class="dual" class:low-top={at(lo) > 50}>
  <div class="fill" style:left="{at(lo)}%" style:width="{at(hi) - at(lo)}%"></div>
  <input
    class="lo"
    type="range"
    {min}
    {max}
    {step}
    value={lo}
    oninput={(e) => {
      const v = Math.min(Number(e.currentTarget.value), hi);
      e.currentTarget.value = String(v);
      oninput(v, hi);
    }}
    {onchange}
    aria-label={labelLo}
  />
  <input
    class="hi"
    type="range"
    {min}
    {max}
    {step}
    value={hi}
    oninput={(e) => {
      const v = Math.max(Number(e.currentTarget.value), lo);
      e.currentTarget.value = String(v);
      oninput(lo, v);
    }}
    {onchange}
    aria-label={labelHi}
  />
</div>

<style>
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
    .dual {
      height: 44px;
    }
  }
</style>
