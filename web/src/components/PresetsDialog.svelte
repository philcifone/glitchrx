<script lang="ts">
  // Presets: recipes saved on the server under a name, shared by every
  // device that uses it. Tap one to use it (undo brings the old recipe
  // back), or add its steps to the current recipe.
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { app, type NewPhotoChoice } from '../lib/app.svelte';

  let dialog: HTMLDialogElement;
  let name = $state('');

  export function show(): void {
    void app.loadPresets();
    dialog.showModal();
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (await app.savePreset(name)) name = '';
  }

  function use(preset: string, append: boolean) {
    dialog.close();
    void app.applyPreset(preset, append);
  }

  function remove(preset: string) {
    if (confirm(`Delete the preset "${preset}"? This cannot be undone.`)) void app.deletePreset(preset);
  }

  const NEW_PHOTO: { id: NewPhotoChoice; label: string }[] = [
    { id: 'ask', label: 'Ask' },
    { id: 'keep', label: 'Keep steps' },
    { id: 'fresh', label: 'Start fresh' },
  ];

  const canSave = $derived(name.trim() !== '' && app.recipe.steps.some((s) => s.enabled));
  // A preset's effects for its row: the first few, then "...".
  const summary = (effects: string[]) => {
    const shown = effects.slice(0, 4).join(', ');
    return effects.length > 4 ? `${shown}, ...` : shown;
  };
</script>

<dialog bind:this={dialog} aria-labelledby="presets-title">
  <div class="head">
    <h2 id="presets-title">Presets</h2>
    <button class="plain" onclick={() => dialog.close()}>Done</button>
  </div>

  <div class="body">
    <h3 class="section-label">Save this recipe</h3>
    <form class="group save" onsubmit={save}>
      <input
        type="text"
        bind:value={name}
        placeholder="Name"
        maxlength="60"
        autocomplete="off"
        spellcheck="false"
        aria-label="Preset name"
      />
      <button class="primary" type="submit" disabled={!canSave}>Save</button>
    </form>
    <p class="dim small note">
      Steps, settings, seeds and masks, saved on this glitchrx server, so every device using it sees them.
    </p>

    <h3 class="section-label">Your presets</h3>
    {#if app.presets.length === 0}
      <p class="dim small note">None yet. Build a recipe you like and save it above.</p>
    {:else}
      <ul class="group list">
        {#each app.presets as p (p.name)}
          <li>
            <button class="use" onclick={() => use(p.name, false)} disabled={!!p.error} title={p.error ?? `Use ${p.name} (replaces the steps; undo brings them back)`}>
              <span class="name">{p.name}</span>
              {#if p.error}
                <span class="small err">{p.error}</span>
              {:else}
                <span class="small dim">{p.steps} {p.steps === 1 ? 'step' : 'steps'}: {summary(p.effects)}</span>
              {/if}
            </button>
            <button class="icon" onclick={() => use(p.name, true)} disabled={!!p.error} title="Add its steps after the current ones" aria-label="Add the steps of {p.name}">
              <ListPlus size={18} strokeWidth={1.75} />
            </button>
            <button class="icon danger" onclick={() => remove(p.name)} title="Delete" aria-label="Delete {p.name}">
              <Trash2 size={18} strokeWidth={1.75} />
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <h3 class="section-label">When you open another photo</h3>
    <div class="segmented" role="radiogroup" aria-label="When you open another photo">
      {#each NEW_PHOTO as c (c.id)}
        <button role="radio" aria-checked={app.newPhoto === c.id} class:on={app.newPhoto === c.id} onclick={() => app.setNewPhotoChoice(c.id)}>
          {c.label}
        </button>
      {/each}
    </div>
    <p class="dim small note">What happens to the steps already in the stack. Applies to this device.</p>
  </div>
</dialog>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 10px 10px var(--pad);
    border-bottom: var(--hairline);
  }

  .head h2 {
    font-size: 1.2rem;
  }

  .body {
    padding: 8px var(--pad) var(--pad);
    display: flex;
    flex-direction: column;
    gap: 8px;
    overflow-y: auto;
    max-height: calc(100dvh - 120px);
    background: var(--bg);
  }

  .section-label {
    margin: 10px 4px 0;
  }

  .group {
    background: var(--card);
    border-radius: 12px;
  }

  .save {
    display: flex;
    gap: 8px;
    padding: 10px;
  }

  .save input {
    flex: 1;
    min-width: 0;
  }

  .note {
    margin: 0 4px;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0 6px 0 0;
  }

  .list li {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .list li + li {
    border-top: var(--hairline);
  }

  .use {
    flex: 1;
    min-width: 0;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    background: none;
    padding: 10px 14px;
    text-align: left;
    border-radius: 12px;
  }

  .use span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name {
    font-size: 1rem;
  }

  .small {
    font-size: 0.82rem;
    font-weight: 400;
  }

  .err {
    color: var(--danger);
  }
</style>
