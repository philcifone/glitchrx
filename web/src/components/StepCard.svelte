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
  import { hold, tip } from '../lib/hold';
  import { shakes, usesSamples } from '../lib/palette';
  import { CHANNEL_CHOICES, channelName } from '../lib/channels';
  import type { Channels, StepState } from '../lib/types';
  import ParamField from './ParamField.svelte';
  import LevelsField from './LevelsField.svelte';
  import BlendSection from './BlendSection.svelte';
  import PullSection from './PullSection.svelte';

  let { step, index }: { step: StepState; index: number } = $props();

  const info = $derived(app.effect(step.effect));
  const own = $derived(info?.params.filter((p) => !p.placement) ?? []);
  // Levels gets one editor for its points (histogram and handles, as in
  // Photoshop); its other settings (channel) stay ordinary fields.
  const LEVEL_POINTS = ['black', 'white', 'gamma', 'out_black', 'out_white'];
  const levels = $derived(step.effect === 'levels' ? own.filter((p) => LEVEL_POINTS.includes(p.name)) : []);
  const fields = $derived(levels.length ? own.filter((p) => !LEVEL_POINTS.includes(p.name)) : own);
  const placement = $derived(info?.params.filter((p) => p.placement) ?? []);
  /** Drawn by hand (pull): strokes take the place of placement and seed. */
  const drawn = $derived(!!info?.strokes);
  const seeded = $derived(app.seeded(step));
  const banded = $derived(app.banded(step));
  // Hits and length only mean something with bands on (D-56).
  const shownPlacement = $derived(placement.filter((p) => banded || (p.name !== 'hits' && p.name !== 'len')));
  const Icon = $derived(iconFor(step.effect));
  const encodings = $derived(app.config?.encodings ?? []);
  // Empty string = follow the recipe.
  const stepEncoding = $derived(step.params[ENCODING_KEY] ?? '');
  const stepChannels = $derived(step.params[CHANNELS_KEY] ?? '');
  const opacity = $derived(app.opacity(step));
  // Open when first added. Collapsing is per card and not part of history;
  // the app keeps it, so it survives the cards being rebuilt (Edit mode).
  const open = $derived(!app.collapsed[step.uid]);

  // Seed field edits are applied on change; typing is not live.
  let seedText = $state('');
  $effect(() => {
    seedText = step.seed;
  });
</script>

<article class:off={!step.enabled}>
  <!-- Held (not on the switch): Edit mode for the whole list. -->
  <header {@attach hold(() => app.startEditingSteps(), { ignore: '.toggle' })}>
    <span class="n">{index + 1}</span>
    <button class="name" onclick={() => app.toggleCollapsed(step.uid)} aria-expanded={open} title={info?.summary}>
      <span class="glyph"><Icon size={18} strokeWidth={1.75} /></span>
      <!-- The number in the system font: Young Serif's old-style 1 reads as "i". -->
      <span class="label serif">{step.effect}{#if app.stepNumber(step)}<span class="num">-{app.stepNumber(step)}</span>{/if}</span>
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
  </header>

  {#if open}
    <div class="body">
      {#if info}
        <section class="group">
          <!-- Inside the card, not on its header, where it was easy to bump. -->
          {#if own.length}<div class="shake-row">
            <h3 class="section-label">Settings</h3>
            {#if shakes(info) && !drawn}
              <button class="plain" onclick={() => app.shake(step.uid)} title="Random settings for this step (placement stays)" aria-label="Shake settings">
                <Shuffle size={16} strokeWidth={1.75} /> Shake
              </button>
            {/if}
          </div>{/if}
          {#each fields as spec (spec.name)}
            <ParamField uid={step.uid} {spec} value={step.params[spec.name]} />
          {/each}
          {#if levels.length}
            <LevelsField uid={step.uid} specs={levels} params={step.params} />
          {/if}
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
        {#if drawn}
        <PullSection {step} />
        {:else}
        <section class="group" aria-label="Placement">
          <h3 class="section-label">Placement</h3>
          <!-- Off: the step covers its whole region. On: it lands in seeded
               bands (hits), which the seed places. -->
          <label class="bands">
            <span>Bands</span>
            <span class="dim note">{banded ? 'scattered in seeded bands' : 'the whole picture'}</span>
            <input class="switch" type="checkbox" checked={banded} onchange={(e) => app.setBands(step.uid, e.currentTarget.checked)} />
          </label>
          {#each shownPlacement as spec (spec.name)}
            <ParamField uid={step.uid} {spec} value={step.params[spec.name]} />
          {/each}
          <!-- The seed decides where the hits land, so it lives here, with
               the dice that re-rolls it. Without bands it only matters to
               effects that draw inside their window (shuffle). -->
          {#if seeded}<div class="seed">
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
          </div>{/if}
        </section>
        {/if}
        <BlendSection {step} />
      {:else}
        <p class="err">Unknown effect `{step.effect}`.</p>
      {/if}

      <!-- What to do with the step itself, apart from its settings: icons
           only (Phil, 2026-11-24); hold one for its name. -->
      <div class="foot" role="group" aria-label="Step">
        <button class="icon" {@attach tip} onclick={() => app.moveStep(step.uid, -1)} disabled={!app.canMoveStep(step.uid, -1)} title="Move up" aria-label="Move up">
          <ChevronUp size={20} strokeWidth={1.75} />
        </button>
        <button class="icon" {@attach tip} onclick={() => app.moveStep(step.uid, 1)} disabled={!app.canMoveStep(step.uid, 1)} title="Move down" aria-label="Move down">
          <ChevronDown size={20} strokeWidth={1.75} />
        </button>
        <button class="icon" {@attach tip} onclick={() => app.duplicateStep(step.uid)} title="Duplicate (with a new seed)" aria-label="Duplicate">
          <Copy size={18} strokeWidth={1.75} />
        </button>
        <span class="grow"></span>
        <button class="icon danger" {@attach tip} onclick={() => app.removeStep(step.uid)} title="Remove this step" aria-label="Remove step">
          <Trash2 size={18} strokeWidth={1.75} />
        </button>
      </div>
    </div>
  {/if}
</article>

<style>
  .num {
    font-family: var(--font);
    font-variant-numeric: lining-nums tabular-nums;
    font-size: 0.9em;
  }

  .shake-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin: -4px 0 2px;
  }

  .shake-row .plain {
    gap: 6px;
    font-size: 0.9rem;
    min-height: 34px;
  }

  /* A grouped card, as in iOS Settings. */
  article {
    background: var(--card);
    border-radius: var(--radius-lg);
    overflow: hidden;
    transition:
      opacity 0.2s,
      transform 0.15s;
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
    gap: 6px;
    padding: 6px 0;
  }

  .foot .grow {
    flex: 1;
  }

  /* Holding the header is a gesture (Edit mode): no text selection or
     system menu, and the card sinks a little while held, as on iOS. */
  header {
    -webkit-user-select: none;
    user-select: none;
    -webkit-touch-callout: none;
  }

  article:has(> header:global(.pressing)) {
    transform: scale(0.98);
    transition:
      opacity 0.2s,
      transform 0.45s ease-in;
  }

  .bands {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.9rem;
    font-weight: 500;
  }

  .bands .note {
    flex: 1;
    min-width: 0;
    font-size: 0.75rem;
    font-weight: 400;
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
