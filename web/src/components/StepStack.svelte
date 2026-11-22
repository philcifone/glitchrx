<script lang="ts">
  // The step stack: the recipe-wide sample settings (Audacity's Import Raw
  // Data choices), the effect palette, and the steps in order. On phones
  // the palette moves into a sheet behind a floating "+" button, so the
  // steps get the room.
  import Dices from '@lucide/svelte/icons/dices';
  import Plus from '@lucide/svelte/icons/plus';
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { CHANNEL_CHOICES } from '../lib/channels';
  import EffectPalette from './EffectPalette.svelte';
  import StepCard from './StepCard.svelte';

  const encodings = $derived(app.config?.encodings ?? []);
  const current = $derived(encodings.find((e) => e.id === app.recipe.encoding));

  let list: HTMLOListElement;
  let sheet: HTMLDialogElement;

  /** Add a step and bring its card into view. */
  async function add(effect: string) {
    if (sheet?.open) sheet.close();
    app.addStep(effect);
    await tick();
    list?.lastElementChild?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
  }

  /** A tap on the backdrop (the dialog element itself) closes the sheet. */
  function backdrop(e: MouseEvent) {
    if (e.target === sheet) sheet.close();
  }
</script>

<div class="wrap">
<div class="stack">
  <section class="card sample">
    <div class="row">
      <label for="encoding">Encoding</label>
      <select id="encoding" value={app.recipe.encoding} onchange={(e) => app.setEncoding(e.currentTarget.value)}>
        {#each encodings as enc (enc.id)}
          <option value={enc.id}>{enc.id}</option>
        {/each}
      </select>
      <div class="segmented" role="radiogroup" aria-label="Channels">
        {#each CHANNEL_CHOICES as [n, name, title] (n)}
          <button
            role="radio"
            aria-checked={app.recipe.channels === n}
            class:on={app.recipe.channels === n}
            onclick={() => app.setChannels(n)}
            {title}
          >
            {name}
          </button>
        {/each}
      </div>
    </div>
    {#if current}
      <p class="dim note">{current.describe}. Any step can override it.</p>
    {/if}
  </section>

  <div class="palette">
    <EffectPalette onpick={add} />
  </div>

  <div class="head">
    <h2 class="section-label">Steps</h2>
    <button class="icon" onclick={() => app.rerollAll()} disabled={!app.anySeeded} title="New seeds for every step that has something random" aria-label="Re-roll all seeds">
      <Dices size={18} strokeWidth={1.75} />
    </button>
    <button class="plain" onclick={() => app.clearSteps()} disabled={app.recipe.steps.length === 0}>Clear</button>
  </div>

  <ol class="steps" bind:this={list}>
    {#each app.recipe.steps as step, i (step.uid)}
      <li><StepCard {step} index={i} count={app.recipe.steps.length} /></li>
    {:else}
      <li class="dim none">
        No steps yet. <span class="wide-only">Pick an effect above</span><span class="phone-only">Tap + to add an effect</span>;
        steps run top to bottom.
      </li>
    {/each}
  </ol>
</div>

<button class="primary fab" onclick={() => sheet.showModal()} disabled={!app.config} aria-label="Add an effect" title="Add an effect">
  <Plus size={26} strokeWidth={2} />
</button>
</div>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<dialog class="add-sheet" bind:this={sheet} onclick={backdrop} aria-labelledby="add-title">
  <div class="sheet-head">
    <h2 id="add-title">Add an effect</h2>
    <button class="plain" onclick={() => sheet.close()}>Cancel</button>
  </div>
  <div class="sheet-body">
    <EffectPalette onpick={add} />
  </div>
</dialog>

<style>
  .stack {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    scrollbar-width: thin;
    scrollbar-color: var(--fill-strong) transparent;
    padding: 12px var(--pad);
    padding-bottom: calc(var(--pad) + env(safe-area-inset-bottom));
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .card {
    background: var(--card);
    border-radius: var(--radius-lg);
    padding: 12px 14px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .row select {
    flex: 1;
    min-width: 0;
  }

  .sample label {
    font-weight: 600;
  }

  .note {
    margin: 8px 0 0;
    font-size: 0.85rem;
  }

  /* Encoding descriptions come from the server in lower case. */
  .note::first-letter {
    text-transform: uppercase;
  }

  .segmented {
    flex: none;
  }

  .palette {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  /* The wrapper keeps the floating button out of the scrolling area. */
  .wrap {
    flex: 1;
    min-height: 0;
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .fab,
  .phone-only {
    display: none;
  }

  /* Add sheet: a dialog pinned to the bottom of the screen. */
  .add-sheet {
    margin: auto auto 0;
    width: 100%;
    max-width: 560px;
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    padding-bottom: env(safe-area-inset-bottom);
    background: var(--bg);
  }

  .add-sheet[open] {
    animation: rise 0.22s ease-out;
  }

  @keyframes rise {
    from {
      transform: translateY(100%);
    }
  }

  .sheet-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 10px 4px var(--pad);
  }

  .sheet-head h2 {
    font-size: 1.2rem;
  }

  .sheet-body {
    padding: 8px var(--pad) var(--pad);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  @media (max-width: 760px) {
    .palette,
    .wide-only {
      display: none;
    }

    .phone-only {
      display: inline;
    }

    /* Leave room under the last card for the floating button. */
    .stack {
      padding-bottom: calc(84px + env(safe-area-inset-bottom));
    }

    .fab {
      display: inline-flex;
      position: absolute;
      right: var(--pad);
      bottom: calc(var(--pad) + env(safe-area-inset-bottom));
      width: 56px;
      height: 56px;
      padding: 0;
      border-radius: 50%;
      box-shadow: 0 4px 14px rgb(0 0 0 / 0.3);
      z-index: 2;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .add-sheet[open] {
      animation: none;
    }
  }

  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 0 0 4px;
  }

  .head h2 {
    flex: 1;
  }

  .steps {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .none {
    padding: 4px;
    font-size: 0.9rem;
  }
</style>
