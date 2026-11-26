<script lang="ts">
  // The venetian mask as a switch (D-75): filled with the accent while the
  // mask is used, struck through and dim while it is off. The same control
  // in three places: a step's header (its whole mask), the Blend section
  // beside Add to mask (the same), and each part of the mask (that part).
  // The chip is small; the button around it is the touch target.
  import VenetianMask from '@lucide/svelte/icons/venetian-mask';
  import { OFF_PATHS } from '../lib/icons';
  import Slashed from './Slashed.svelte';

  let {
    off,
    what,
    onclick,
    disabled = false,
  }: { off: boolean; what: string; onclick: () => void; disabled?: boolean } = $props();
</script>

<button
  class="mask-toggle"
  class:off
  {onclick}
  {disabled}
  aria-pressed={!off}
  aria-label="Use {what}"
  title={off ? `${what[0].toUpperCase()}${what.slice(1)} is off: tap to use it again` : `Switch ${what} off (it is kept)`}
>
  <span class="chip">
    {#if off}<Slashed paths={OFF_PATHS.venetianMask} size={14} />{:else}<VenetianMask size={14} strokeWidth={1.75} />{/if}
  </span>
</button>

<style>
  .mask-toggle {
    flex: none;
    min-height: 32px;
    min-width: 32px;
    padding: 0 2px;
    background: none;
    border: none;
  }

  .mask-toggle:hover:not(:disabled) {
    background: none;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    padding: 2px 5px;
    border-radius: 5px;
    color: var(--accent-fg);
    background: var(--accent);
  }

  .off .chip {
    color: var(--fg-dim);
    background: var(--fill);
  }
</style>
