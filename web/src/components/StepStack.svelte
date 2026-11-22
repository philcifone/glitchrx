<script lang="ts">
  // The step stack: the recipe-wide sample settings (Audacity's Import Raw
  // Data choices), the effect palette, and the steps in order.
  import Dices from '@lucide/svelte/icons/dices';
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { iconFor } from '../lib/icons';
  import type { Channels } from '../lib/types';
  import StepCard from './StepCard.svelte';

  const encodings = $derived(app.config?.encodings ?? []);
  const current = $derived(encodings.find((e) => e.id === app.recipe.encoding));

  let list: HTMLOListElement;

  /** Add a step and bring its card into view (the palette sits above). */
  async function add(effect: string) {
    app.addStep(effect);
    await tick();
    list?.lastElementChild?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }
</script>

<div class="stack">
  <section class="sample">
    <div class="row">
      <label for="encoding">Encoding</label>
      <select id="encoding" value={app.recipe.encoding} onchange={(e) => app.setEncoding(e.currentTarget.value)}>
        {#each encodings as enc (enc.id)}
          <option value={enc.id}>{enc.id}</option>
        {/each}
      </select>
      <div class="seg" role="radiogroup" aria-label="Channels">
        {#each [1, 2] as n (n)}
          <button
            role="radio"
            aria-checked={app.recipe.channels === n}
            class:on={app.recipe.channels === n}
            onclick={() => app.setChannels(n as Channels)}
            title={n === 1 ? 'Mono: every sample in one stream' : 'Stereo: alternate samples are left and right, processed separately'}
          >
            {n === 1 ? 'mono' : 'stereo'}
          </button>
        {/each}
      </div>
    </div>
    {#if current}
      <p class="dim note">{current.describe}. Any step can override it.</p>
    {/if}
  </section>

  <section class="palette" aria-label="Add an effect">
    {#each app.effects as e (e.id)}
      {@const Icon = iconFor(e.id)}
      <button onclick={() => add(e.id)} title="{e.summary} (Audacity: {e.audacity})" disabled={!app.config}>
        <Icon size={24} strokeWidth={2.25} />
        <span>{e.id}</span>
      </button>
    {/each}
  </section>

  <div class="head">
    <h2>Steps</h2>
    <button class="icon" onclick={() => app.rerollAll()} disabled={!app.anySeeded} title="New seeds for every step that has something random" aria-label="Re-roll all seeds">
      <Dices size={18} strokeWidth={2.25} />
    </button>
    <button onclick={() => app.clearSteps()} disabled={app.recipe.steps.length === 0}>Clear</button>
  </div>

  <ol class="steps" bind:this={list}>
    {#each app.recipe.steps as step, i (step.uid)}
      <li><StepCard {step} index={i} count={app.recipe.steps.length} /></li>
    {:else}
      <li class="dim none">No steps yet. Tap an effect above; steps run top to bottom.</li>
    {/each}
  </ol>
</div>

<style>
  .stack {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: var(--pad);
    padding-bottom: calc(var(--pad) + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: var(--pad);
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .row select {
    flex: 1;
    min-width: 0;
  }

  .sample label {
    font-weight: 700;
  }

  .note {
    margin: 6px 0 0;
    font-size: 0.88em;
  }

  /* Segmented mono/stereo switch: two buttons sharing one border. */
  .seg {
    display: flex;
    flex: none;
  }

  .seg button {
    padding: 0 10px;
  }

  .seg button + button {
    border-left: none;
  }

  .seg button.on {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }

  /* Effect palette: one tap adds a step. Five columns, so the ten effects
     make two rows even on a phone. */
  .palette {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 6px;
  }

  .palette button {
    flex-direction: column;
    gap: 4px;
    padding: 8px 2px 6px;
    min-height: 64px;
  }

  .palette button span {
    font-size: 0.68rem;
    letter-spacing: -0.05em;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .palette button:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    border-top: var(--border);
    padding-top: var(--pad);
  }

  .head h2 {
    flex: 1;
    font-size: 1.25rem;
  }

  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .none {
    padding: 8px 0;
  }
</style>
