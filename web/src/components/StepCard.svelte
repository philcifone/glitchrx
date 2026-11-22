<script lang="ts">
  // One step of the recipe: header (icon, name, on/off, shake, re-roll)
  // and, when open, its parameters, sample settings, placement (always
  // shown: it is where most of the variety comes from), seed and
  // arrangement buttons.
  import Dices from '@lucide/svelte/icons/dices';
  import Shuffle from '@lucide/svelte/icons/shuffle';
  import VenetianMask from '@lucide/svelte/icons/venetian-mask';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Copy from '@lucide/svelte/icons/copy';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { app, CHANNELS_KEY, ENCODING_KEY } from '../lib/app.svelte';
  import { iconFor } from '../lib/icons';
  import { usesSamples } from '../lib/palette';
  import { CHANNEL_CHOICES, channelName } from '../lib/channels';
  import type { Channels, StepState } from '../lib/types';
  import ParamField from './ParamField.svelte';
  import BlendSection from './BlendSection.svelte';

  let { step, index, count }: { step: StepState; index: number; count: number } = $props();

  const info = $derived(app.effect(step.effect));
  const own = $derived(info?.params.filter((p) => !p.placement) ?? []);
  const placement = $derived(info?.params.filter((p) => p.placement) ?? []);
  const seeded = $derived(app.seeded(step));
  const Icon = $derived(iconFor(step.effect));
  const encodings = $derived(app.config?.encodings ?? []);
  // Empty string = follow the recipe.
  const stepEncoding = $derived(step.params[ENCODING_KEY] ?? '');
  const stepChannels = $derived(step.params[CHANNELS_KEY] ?? '');
  const opacity = $derived(app.opacity(step));
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
      <span class="glyph"><Icon size={18} strokeWidth={1.75} /></span>
      <span class="label serif">{step.effect}</span>
      {#if stepEncoding}<span class="enc">{stepEncoding}</span>{/if}
      {#if step.mask}
        <span class="enc tag-icon" class:live={app.masking === step.uid} title="Masked" aria-label="Masked">
          <VenetianMask size={14} strokeWidth={1.75} />
        </span>
      {/if}
      {#if opacity < 1}<span class="enc">{Math.round(opacity * 100)}%</span>{/if}
    </button>
    <label class="toggle" title={step.enabled ? 'Turn this step off' : 'Turn this step on'}>
      <input class="switch" type="checkbox" checked={step.enabled} onchange={() => app.toggleStep(step.uid)} />
      <span class="visually-hidden">Enabled</span>
    </label>
    <button class="icon" onclick={() => app.shake(step.uid)} disabled={!info} title="Shake: random settings for this step (placement stays)" aria-label="Shake settings">
      <Shuffle size={18} strokeWidth={1.75} />
    </button>
  </header>

  {#if open}
    <div class="body">
      {#if info}
        <section class="group">
          {#each own as spec (spec.name)}
            <ParamField uid={step.uid} {spec} value={step.params[spec.name]} />
          {/each}
        </section>
        {#if usesSamples(info)}
        <div class="sample">
          <label>
            <span>Encoding</span>
            <select value={stepEncoding} onchange={(e) => app.setStepEncoding(step.uid, e.currentTarget.value || null)}>
              <option value="">Recipe ({app.recipe.encoding})</option>
              {#each encodings as enc (enc.id)}
                <option value={enc.id}>{enc.id}</option>
              {/each}
            </select>
          </label>
          <label>
            <span>Channels</span>
            <select
              value={stepChannels}
              onchange={(e) => {
                const v = e.currentTarget.value;
                app.setStepChannels(step.uid, v ? (Number(v) as Channels) : null);
              }}
            >
              <option value="">Recipe ({channelName(app.recipe.channels)})</option>
              {#each CHANNEL_CHOICES as [n, name] (n)}
                <option value={String(n)}>{name}</option>
              {/each}
            </select>
          </label>
        </div>
        {/if}
        <section class="group" aria-label="Placement">
          <h3 class="section-label">Placement</h3>
          {#each placement as spec (spec.name)}
            <ParamField uid={step.uid} {spec} value={step.params[spec.name]} />
          {/each}
          <!-- The seed decides where the hits land, so it lives here, with
               the dice that re-rolls it. -->
          <div class="seed">
            <label for="seed-{step.uid}">Seed</label>
            <input
              id="seed-{step.uid}"
              type="text"
              inputmode="numeric"
              bind:value={seedText}
              onchange={() => app.setSeed(step.uid, seedText)}
              spellcheck="false"
              autocomplete="off"
            />
            <button
              class="icon"
              onclick={() => app.reroll(step.uid)}
              disabled={!seeded}
              title={seeded
                ? 'New seed: the same settings, landing somewhere else'
                : 'Nothing random here: with hits=0 the step covers its whole region. Set hits above 0 to scatter it.'}
              aria-label="Re-roll seed"
            >
              <Dices size={18} strokeWidth={1.75} />
            </button>
          </div>
        </section>
        <BlendSection {step} />
      {:else}
        <p class="err">Unknown effect `{step.effect}`.</p>
      {/if}

      <!-- What to do with the step itself, apart from its settings. -->
      <div class="foot" role="group" aria-label="Step">
        <button class="plain" onclick={() => app.moveStep(step.uid, -1)} disabled={index === 0} title="Move up" aria-label="Move up">
          <ChevronUp size={18} strokeWidth={1.75} /> <span class="txt">Up</span>
        </button>
        <button class="plain" onclick={() => app.moveStep(step.uid, 1)} disabled={index === count - 1} title="Move down" aria-label="Move down">
          <ChevronDown size={18} strokeWidth={1.75} /> <span class="txt">Down</span>
        </button>
        <button class="plain" onclick={() => app.duplicateStep(step.uid)} title="Duplicate (with a new seed)" aria-label="Duplicate">
          <Copy size={17} strokeWidth={1.75} /> <span class="txt">Duplicate</span>
        </button>
        <span class="grow"></span>
        <button class="plain danger" onclick={() => app.removeStep(step.uid)} title="Remove this step" aria-label="Remove step">
          <Trash2 size={17} strokeWidth={1.75} /> <span class="txt">Remove</span>
        </button>
      </div>
    </div>
  {/if}
</article>

<style>
  /* A grouped card, as in iOS Settings. */
  article {
    background: var(--card);
    border-radius: var(--radius-lg);
    overflow: hidden;
    transition: opacity 0.2s;
  }

  article.off {
    opacity: 0.5;
  }

  header {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 6px 6px 12px;
  }

  .n {
    min-width: 1.6em;
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--fg-dim);
    font-variant-numeric: tabular-nums;
  }

  .name {
    flex: 1;
    min-width: 0;
    justify-content: flex-start;
    gap: 8px;
    background: none;
    padding: 4px 2px;
    font-size: 1.05rem;
  }

  .name:hover:not(:disabled) {
    background: none;
  }

  .glyph {
    display: inline-flex;
    color: var(--accent);
  }

  .label {
    text-transform: capitalize;
  }

  /* An encoding override, as a small tag. */
  .enc {
    font-size: 0.72rem;
    font-weight: 500;
    color: var(--fg-dim);
    background: var(--fill);
    padding: 1px 6px;
    border-radius: 5px;
  }

  .enc.tag-icon {
    display: inline-flex;
    align-items: center;
    padding: 2px 5px;
  }

  .enc.live {
    color: var(--accent-fg);
    background: var(--accent);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0 6px;
    min-height: 36px;
    cursor: pointer;
  }

  .body {
    padding: 0 14px 4px;
    display: flex;
    flex-direction: column;
  }

  /* Sections inside the card, divided by hairlines. */
  .group,
  .sample,
  .foot {
    border-top: var(--hairline);
    padding: 12px 0;
  }

  .group {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .sample {
    display: flex;
    gap: 10px;
  }

  .sample label {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .sample span,
  .seed label {
    font-size: 0.85rem;
    font-weight: 500;
  }

  .foot {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 6px 0;
  }

  .foot button {
    gap: 4px;
    padding: 6px 8px;
    font-size: 0.85rem;
  }

  .foot .grow {
    flex: 1;
  }

  /* The narrowest phones get icons only (each button keeps its name). */
  @media (max-width: 360px) {
    .foot .txt {
      display: none;
    }
  }

  .seed {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .seed input {
    flex: 1;
    min-width: 0;
    width: 100%;
  }

  .err {
    color: var(--danger);
    margin: 0;
    padding: 12px 0;
  }

  @media (pointer: coarse) {
    .toggle {
      min-height: var(--touch);
    }
  }
</style>
