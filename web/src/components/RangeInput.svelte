<script lang="ts">
  // A slider over 0..SLIDER_STEPS positions that knows where its default is:
  //   - a small tick under the track marks the default
  //   - dragging near it snaps onto it (lightly: within SNAP positions)
  //   - a double tap (or double click) puts it back on the default
  // The parent maps positions to values; this only deals in positions.
  import { SLIDER_STEPS } from '../lib/slider';

  let {
    pos,
    defaultPos = null,
    label,
    oninput,
    onchange,
    onreset,
  }: {
    /** Where the thumb is, 0..SLIDER_STEPS. */
    pos: number;
    /** Where the default sits, or null if it has no place on this slider. */
    defaultPos?: number | null;
    label: string;
    /** Live position while dragging. */
    oninput: (pos: number) => void;
    /** The drag ended. */
    onchange: () => void;
    /** Double tap: back to the default. */
    onreset: () => void;
  } = $props();

  /** How close counts as "on" the default: 1.2% of the track. */
  const SNAP = 12;
  /** Two taps this close in time (ms) and space (px) make a double tap. */
  const DOUBLE_MS = 300;
  const DOUBLE_PX = 24;

  const fraction = (p: number) => Math.min(1, Math.max(0, p / SLIDER_STEPS));

  /**
   * Set by a double tap until that finger lifts: the second tap still
   * moves the thumb under it, which would undo the reset.
   */
  let resetting = false;

  function input(e: Event & { currentTarget: HTMLInputElement }) {
    if (resetting) {
      e.currentTarget.value = String(pos);
      return;
    }
    let p = Number(e.currentTarget.value);
    if (defaultPos !== null && Math.abs(p - defaultPos) <= SNAP) {
      p = defaultPos;
      e.currentTarget.value = String(p); // the thumb sits on the tick
    }
    oninput(p);
  }

  // Our own double-tap detection: iOS does not reliably send dblclick to a
  // range input, and the first tap moves the thumb anyway, so a reset on
  // the second one is what the finger expects.
  let lastTap = { t: 0, x: 0, y: 0 };
  function pointerdown(e: PointerEvent) {
    const now = e.timeStamp;
    const near = Math.hypot(e.clientX - lastTap.x, e.clientY - lastTap.y) <= DOUBLE_PX;
    if (now - lastTap.t <= DOUBLE_MS && near) {
      lastTap = { t: 0, x: 0, y: 0 };
      e.preventDefault();
      resetting = true;
      onreset();
      return;
    }
    lastTap = { t: now, x: e.clientX, y: e.clientY };
  }

  // After the lifting finger's own change event, which comes first.
  const release = () => setTimeout(() => (resetting = false), 0);
</script>

<span class="wrap">
  <input
    type="range"
    min="0"
    max={SLIDER_STEPS}
    step="1"
    value={pos}
    style:--pct="{fraction(pos) * 100}%"
    oninput={input}
    onchange={(e) => {
      if (resetting) e.currentTarget.value = String(pos);
      else onchange();
    }}
    onpointerdown={pointerdown}
    onpointerup={release}
    onpointercancel={release}
    aria-label={label}
    title="Double tap for the default"
  />
  {#if defaultPos !== null}
    <!-- The thumb is 22 px wide, so its centre runs from 11 px to 100% - 11 px. -->
    <span class="tick" style:left="calc(11px + (100% - 22px) * {fraction(defaultPos)})" aria-hidden="true"></span>
  {/if}
</span>

<style>
  .wrap {
    position: relative;
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
  }

  input {
    /* No double-tap zoom on touch screens: double tap resets instead. */
    touch-action: manipulation;
  }

  .tick {
    position: absolute;
    top: calc(50% + 5px);
    width: 2px;
    height: 6px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--fg-dim);
    opacity: 0.7;
    pointer-events: none;
  }
</style>
