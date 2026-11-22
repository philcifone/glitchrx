<script lang="ts">
  // One step of the recipe: header (name, on/off, re-roll) and, when open,
  // its parameters, placement, seed and arrangement buttons.
  import Dices from '@lucide/svelte/icons/dices';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Copy from '@lucide/svelte/icons/copy';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../lib/app.svelte';
  import type { StepState } from '../lib/types';
  import ParamField from './ParamField.svelte';

  let { step, index, count }: { step: StepState; index: number; count: number } = $props();

  const info = $derived(app.effect(step.effect));
  const own = $derived(info?.params.filter((p) => !p.placement) ?? []);
  const placement = $derived(info?.params.filter((p) => p.placement) ?? []);
  const changedPlacement = $derived(placement.some((p) => p.name in step.params));
  const seeded = $derived(app.seeded(step));
  // Open when first added; collapsing is per card and not part of history.
  let open = $state(true);

  // Seed field edits are applied on change; typing is not live.
  let seedText = $state('');
  $effect(() => {
    seedText = step.seed;
  });
</script>

<article class:off={!step.enabled}>
  <header>
    <span class="n">{index + 1}</span>
    <button class="name" onclick={() => (open = !open)} aria-expanded={open} title={info?.summary}>
      {step.effect}
    </button>
    <label class="toggle" title={step.enabled ? 'Turn this step off' : 'Turn this step on'}>
      <input type="checkbox" checked={step.enabled} onchange={() => app.toggleStep(step.uid)} />
      <span class="visually-hidden">Enabled</span>
    </label>
    <button
      class="icon"
      onclick={() => app.reroll(step.uid)}
      disabled={!seeded}
      title={seeded
        ? 'New seed for this step'
        : 'Nothing random here: with hits=0 the step covers its whole region. Set hits above 0 (under Placement) to scatter it.'}
      aria-label="Re-roll seed"
    >
      <Dices size={18} strokeWidth={2.25} />
    </button>
  </header>

  {#if open}
    <div class="body">
      {#if info}
        {#each own as spec (spec.name)}
          <ParamField uid={step.uid} {spec} value={step.params[spec.name]} />
        {/each}
        <details open={changedPlacement}>
          <summary>Placement{changedPlacement ? ' (changed)' : ''}</summary>
          {#each placement as spec (spec.name)}
            <ParamField uid={step.uid} {spec} value={step.params[spec.name]} />
          {/each}
        </details>
      {:else}
        <p class="err">Unknown effect `{step.effect}`.</p>
      {/if}

      <div class="foot">
        <label class="seed">
          <span>seed</span>
          <input
            type="text"
            inputmode="numeric"
            bind:value={seedText}
            onchange={() => app.setSeed(step.uid, seedText)}
            spellcheck="false"
            autocomplete="off"
          />
        </label>
        <button class="icon" onclick={() => app.moveStep(step.uid, -1)} disabled={index === 0} aria-label="Move up" title="Move up">
          <ChevronUp size={18} strokeWidth={2.25} />
        </button>
        <button class="icon" onclick={() => app.moveStep(step.uid, 1)} disabled={index === count - 1} aria-label="Move down" title="Move down">
          <ChevronDown size={18} strokeWidth={2.25} />
        </button>
        <button class="icon" onclick={() => app.duplicateStep(step.uid)} aria-label="Duplicate" title="Duplicate (new seed)">
          <Copy size={18} strokeWidth={2.25} />
        </button>
        <button class="icon danger" onclick={() => app.removeStep(step.uid)} aria-label="Remove" title="Remove step">
          <X size={18} strokeWidth={2.5} />
        </button>
      </div>
    </div>
  {/if}
</article>

<style>
  article {
    border: var(--border);
    background: var(--bg);
  }

  article.off {
    opacity: 0.55;
  }

  header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px;
    background: var(--bg1);
  }

  .n {
    min-width: 2ch;
    text-align: center;
    font-weight: 700;
    color: var(--fg-dim);
  }

  .name {
    flex: 1;
    justify-content: flex-start;
    border: none;
    background: none;
    font-family: var(--font-head);
    font-size: 1.15rem;
    padding: 0 4px;
  }

  .name:hover:not(:disabled) {
    background: none;
    text-decoration: underline;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 36px;
    min-height: 36px;
    cursor: pointer;
  }

  .body {
    padding: 8px 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  details summary {
    cursor: pointer;
    font-weight: 700;
    padding: 4px 0;
  }

  details[open] summary {
    margin-bottom: 8px;
  }

  details {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    border-top: 1.5px dashed var(--fg-dim);
    padding-top: 8px;
  }

  .seed {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .seed input {
    flex: 1;
    width: 100%;
  }

  .danger:hover:not(:disabled) {
    color: var(--danger);
    border-color: var(--danger);
  }

  .err {
    color: var(--danger);
    margin: 0;
  }

  @media (pointer: coarse) {
    .toggle {
      min-width: var(--touch);
      min-height: var(--touch);
    }
  }
</style>
