<script lang="ts">
  // "More info" under a card's controls (Phil, 2026-11-26): tap to show a
  // short note inside the card, tap again to hide it. Each card has its
  // own, so the two side by side open independently. Replaces an (i)
  // button whose note appeared under both cards.
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
  let open = $state(false);
</script>

<button class="more" onclick={() => (open = !open)} aria-expanded={open}>
  More info
  {#if open}<ChevronUp size={14} strokeWidth={1.75} />{:else}<ChevronDown size={14} strokeWidth={1.75} />{/if}
</button>
{#if open}
  <p class="note">{@render children()}</p>
{/if}

<style>
  .more {
    align-self: flex-start;
    gap: 4px;
    min-height: 28px;
    padding: 2px 4px;
    background: none;
    color: var(--accent);
    font-size: 0.8rem;
    font-weight: 500;
  }

  .more:hover:not(:disabled) {
    background: none;
  }

  .note {
    margin: 0 4px 2px;
    font-size: 0.8rem;
    color: var(--fg-dim);
  }
</style>
