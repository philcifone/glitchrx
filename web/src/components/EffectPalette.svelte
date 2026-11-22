<script lang="ts">
  // The effect buttons, grouped (audio, data). One tap picks an effect.
  // Used inline in the side panel on wide screens and inside the add
  // sheet on phones.
  import { app } from '../lib/app.svelte';
  import { iconFor } from '../lib/icons';
  import { groupEffects } from '../lib/palette';

  let { onpick }: { onpick: (effect: string) => void } = $props();

  const groups = $derived(groupEffects(app.effects));
</script>

{#each groups as g (g.label)}
  <section class="group" aria-label="{g.label} effects">
    <h3 class="section-label">{g.label}</h3>
    <div class="grid">
      {#each g.effects as e (e.id)}
        {@const Icon = iconFor(e.id)}
        <button onclick={() => onpick(e.id)} title={e.group === 'pixel' ? e.summary : `${e.summary} (Audacity: ${e.audacity})`} disabled={!app.config}>
          <Icon size={22} strokeWidth={1.75} />
          <span>{e.id}</span>
        </button>
      {/each}
    </div>
  </section>
{/each}

<style>
  .group {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .section-label {
    margin-left: 4px;
  }

  /* Five columns, so each group of five is one row even on a phone. */
  .grid {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 8px;
  }

  button {
    flex-direction: column;
    gap: 5px;
    padding: 10px 2px 8px;
    min-height: 62px;
    background: var(--card);
    border-radius: 12px;
    color: var(--accent);
  }

  span {
    color: var(--fg);
    font-size: 0.7rem;
    font-weight: 500;
    text-transform: capitalize;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  @media (hover: hover) {
    button:hover:not(:disabled) {
      background: var(--card);
      box-shadow: inset 0 0 0 1.5px var(--accent-soft);
    }
  }
</style>
