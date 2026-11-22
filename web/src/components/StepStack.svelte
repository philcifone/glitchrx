<script lang="ts">
  // The step stack: file-wide settings (restart bands, mode), the steps in
  // order, and the "add step" picker.
  import Plus from '@lucide/svelte/icons/plus';
  import Dices from '@lucide/svelte/icons/dices';
  import { app, DEFAULT_RESTART } from '../lib/app.svelte';
  import StepCard from './StepCard.svelte';

  let pick = $state('echo');
  const picked = $derived(app.effect(pick));

  // Restart presets; anything else typed in the field is kept as is.
  const presets = ['keep', '5%', '10%', '20%', '0'];
  let restartText = $state('');
  $effect(() => {
    restartText = app.recipe.restart ?? 'keep';
  });

  function setRestart(v: string) {
    const t = v.trim();
    app.setRestart(t === '' || t === 'keep' ? null : t);
  }
</script>

<div class="stack">
  <section class="file">
    <div class="row">
      <label for="restart" title="A restart marker every N% of the image height (or N MCU rows). Each glitch stays inside its band.">
        Bands
      </label>
      <input
        id="restart"
        type="text"
        list="restart-presets"
        bind:value={restartText}
        onchange={() => setRestart(restartText)}
        placeholder={DEFAULT_RESTART}
        autocomplete="off"
        spellcheck="false"
      />
      <datalist id="restart-presets">
        {#each presets as p (p)}<option value={p}></option>{/each}
      </datalist>
      <label for="mode">Mode</label>
      <select id="mode" value={app.recipe.mode} onchange={(e) => app.setMode(e.currentTarget.value as 'safe' | 'raw')}>
        <option value="safe">safe</option>
        <option value="raw">raw</option>
      </select>
    </div>
    <p class="dim note">
      {#if app.recipe.restart === null}
        Bands: keep the file's own restart markers. Damage can run to the bottom.
      {:else if app.recipe.restart === '0'}
        Bands: none. One hit can wash out everything below it.
      {:else}
        Bands every {app.recipe.restart}{app.recipe.restart.endsWith('%') ? ' of the height' : ' MCU rows'}.
      {/if}
      {#if app.recipe.mode === 'raw'}Raw mode can forge markers and truncate; not reversible.{/if}
    </p>
  </section>

  <div class="head">
    <h2>Steps</h2>
    <button class="icon" onclick={() => app.rerollAll()} disabled={app.recipe.steps.length === 0} title="New seeds for every step" aria-label="Re-roll all seeds">
      <Dices size={18} strokeWidth={2.25} />
    </button>
    <button onclick={() => app.clearSteps()} disabled={app.recipe.steps.length === 0}>Clear</button>
  </div>

  <ol class="steps">
    {#each app.recipe.steps as step, i (step.uid)}
      <li><StepCard {step} index={i} count={app.recipe.steps.length} /></li>
    {:else}
      <li class="dim none">No steps yet. Add an effect below; steps run top to bottom.</li>
    {/each}
  </ol>

  <section class="add">
    <div class="row">
      <select bind:value={pick} aria-label="Effect to add">
        {#each app.effects as e (e.id)}
          <option value={e.id}>{e.id}</option>
        {/each}
      </select>
      <button class="primary" onclick={() => app.addStep(pick)} disabled={!picked}>
        <Plus size={18} strokeWidth={2.5} /> Add
      </button>
    </div>
    {#if picked}
      <p class="dim note">{picked.summary}{picked.reversible === 'never' ? ' (lossy)' : ''}</p>
    {/if}
  </section>
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

  .row input,
  .row select {
    flex: 1;
  }

  .file label {
    font-weight: 700;
  }

  .note {
    margin: 6px 0 0;
    font-size: 0.88em;
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

  .add {
    border-top: var(--border);
    padding-top: var(--pad);
  }
</style>
