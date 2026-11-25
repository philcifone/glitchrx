<script lang="ts">
  // The step stack: the recipe-wide sample settings (Audacity's Import Raw
  // Data choices), the effect palette, and the steps in order. On phones
  // the palette moves into a sheet behind a floating "+" button, so the
  // steps get the room.
  import Plus from '@lucide/svelte/icons/plus';
  import ChevronUp from '@lucide/svelte/icons/chevron-up';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Pencil from '@lucide/svelte/icons/pencil';
  import { tip } from '../lib/hold';
  import { blocksOf } from '../lib/groups';
  import { tick } from 'svelte';
  import { app } from '../lib/app.svelte';
  import { revealIn } from '../lib/scroll';
  import { CHANNEL_CHOICES } from '../lib/channels';
  import EffectPalette from './EffectPalette.svelte';
  import StepCard from './StepCard.svelte';
  import StepEditList from './StepEditList.svelte';
  import PhotoTools from './PhotoTools.svelte';

  const encodings = $derived(app.config?.encodings ?? []);
  const current = $derived(encodings.find((e) => e.id === app.recipe.encoding));

  let list = $state<HTMLOListElement>();
  /** Edit mode (held on a step): one line per step, select several, drag to reorder. */
  const editing = $derived(app.editingSteps);

  // Nothing left to edit: back to the cards.
  $effect(() => {
    if (editing && app.recipe.steps.length === 0) app.stopEditingSteps();
  });
  let sheet: HTMLDialogElement;

  /** Add a step and bring its card into view. */
  async function add(effect: string) {
    if (sheet?.open) sheet.close();
    app.addStep(effect);
    await tick();
    revealIn(list?.lastElementChild);
  }

  // The stack as blocks: loose steps, and preset groups holding theirs.
  const blocks = $derived(blocksOf(app.recipe.steps, app.recipe.groups));

  /** A tap on the backdrop (the dialog element itself) closes the sheet. */
  function backdrop(e: MouseEvent) {
    if (e.target === sheet) sheet.close();
  }
</script>

<div class="wrap">
<div class="stack" class:editing>
  {#if app.doc}
    <PhotoTools />
  {/if}
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
    {#if editing}
      <button class="plain strong" onclick={() => app.stopEditingSteps()} title="Back to the cards (or hold a step)">Done</button>
    {:else if app.recipe.steps.length}
      <!-- Fold every card shut or open them all, and Edit mode, which
           holding a step also starts (Phil, 2026-11-25). -->
      <button
        class="icon"
        {@attach tip}
        onclick={() => app.setAllCollapsed(!app.allCollapsed)}
        title={app.allCollapsed ? 'Expand all steps' : 'Collapse all steps'}
        aria-label={app.allCollapsed ? 'Expand all steps' : 'Collapse all steps'}
      >
        {#if app.allCollapsed}<ChevronDown size={20} strokeWidth={1.75} />{:else}<ChevronUp size={20} strokeWidth={1.75} />{/if}
      </button>
      <button
        class="icon"
        {@attach tip}
        onclick={() => app.startEditingSteps()}
        title="Edit the list: select, reorder, duplicate, remove (or hold a step)"
        aria-label="Edit the list of steps"
      >
        <Pencil size={19} strokeWidth={1.75} />
      </button>
    {/if}
  </div>

  {#if editing}
    <StepEditList />
  {:else}

  <ol class="steps" bind:this={list}>
    {#each blocks as b (b.kind === 'step' ? b.step.uid : b.group.id)}
      {#if b.kind === 'step'}
        <li><StepCard step={b.step} index={b.i} /></li>
      {:else}
        {@const id = b.group.id}
        {@const modified = app.groupModified(id)}
        <li class="group" aria-label="From preset {b.group.preset}">
          <div class="ghead">
            <span class="gname">
              <span class="from">From preset</span>
              <span class="serif pname">{b.group.preset}</span>
            </span>
            {#if modified}<span class="badge">Modified</span>{/if}
            <button class="icon" onclick={() => app.moveGroup(id, -1)} disabled={!app.canMoveGroup(id, -1)} title="Move the group up" aria-label="Move the group up">
              <ChevronUp size={18} strokeWidth={1.75} />
            </button>
            <button class="icon" onclick={() => app.moveGroup(id, 1)} disabled={!app.canMoveGroup(id, 1)} title="Move the group down" aria-label="Move the group down">
              <ChevronDown size={18} strokeWidth={1.75} />
            </button>
          </div>
          <ol class="steps inner">
            {#each b.steps as s (s.step.uid)}
              <li><StepCard step={s.step} index={s.i} /></li>
            {/each}
          </ol>
          <div class="gfoot">
            <button class="plain" onclick={() => app.revertGroup(id)} disabled={!modified} title="Back to the preset's settings">Revert</button>
            <button
              class="plain"
              onclick={() => app.updatePreset(id)}
              disabled={!modified || !app.config?.presets}
              title="Save these steps as the preset {b.group.preset}"
            >
              Update preset
            </button>
            <span class="grow"></span>
            <button class="plain" onclick={() => app.ungroup(id)} title="Keep the steps, drop the group">Ungroup</button>
          </div>
        </li>
      {/if}
    {:else}
      <li class="dim none">
        No steps yet. <span class="wide-only">Pick an effect above</span><span class="phone-only">Tap + to add an effect</span>;
        steps run top to bottom.
      </li>
    {/each}
  </ol>
  {/if}
</div>

<!-- Hidden while painting a mask: it would sit on the painting tools. -->
<button
  class="primary fab"
  class:away={app.drawing || editing}
  onclick={() => sheet.showModal()}
  disabled={!app.config}
  aria-label="Add an effect"
  title="Add an effect"
>
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
  /* Edit mode has no floating button to keep room for, and its selection
     bar is sticky, which honours padding: none at the bottom, so the bar
     sits on the edge. */
  .stack.editing {
    padding-bottom: 0;
  }

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

  /* Wraps the channel control onto its own line when the panel is narrow. */
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  .row select {
    flex: 1 1 96px;
    min-width: 96px;
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

    .fab.away {
      display: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .add-sheet[open] {
      animation: none;
    }
  }

  /* As tall with the buttons as with Done, so nothing jumps between modes. */
  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 4px 0 0 4px;
    min-height: 38px;
  }

  .head h2 {
    flex: 1;
  }

  .strong {
    font-weight: 600;
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

  /* A preset group: its cards on a tinted panel, a label above them and
     its actions below. */
  .group {
    background: var(--fill);
    border-radius: calc(var(--radius-lg) + 4px);
    padding: 4px 6px 2px;
  }

  .ghead {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0 6px 8px;
    margin-bottom: 8px;
    border-bottom: var(--hairline);
    min-width: 0;
  }

  .gname {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .from {
    flex: none;
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-dim);
  }

  .pname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .badge {
    flex: none;
    font-size: 0.72rem;
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .gfoot {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    padding: 2px 0;
  }

  .gfoot .plain {
    font-size: 0.9rem;
  }

  .grow {
    flex: 1;
  }
</style>
