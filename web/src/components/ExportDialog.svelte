<script lang="ts">
  // Export: download (or save on the server) at full resolution, and
  // save or load the recipe.
  import Download from '@lucide/svelte/icons/download';
  import Save from '@lucide/svelte/icons/save';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../lib/app.svelte';
  import type { ExportFormat } from '../lib/types';

  let dialog: HTMLDialogElement;
  let recipeInput: HTMLInputElement;
  let format = $state<ExportFormat>('png');
  let withRecipe = $state(true);

  export function show(): void {
    dialog.showModal();
  }

  const formats: { id: ExportFormat; label: string; note: string }[] = [
    {
      id: 'png',
      label: 'Baked PNG',
      note: 'Exactly what you see here, lossless. Looks the same everywhere.',
    },
    {
      id: 'jpeg-baked',
      label: 'Baked JPEG',
      note: 'What you see here, as a clean JPEG (quality 95). Smaller, good for sharing.',
    },
    {
      id: 'jpeg',
      label: 'Glitched JPEG',
      note: 'The broken file itself. Each viewer decodes it its own way, so it may look different elsewhere.',
    },
  ];

  async function run(action: () => Promise<void>) {
    dialog.close();
    await action();
  }

  function loadRecipe() {
    const f = recipeInput.files?.[0];
    if (!f) return;
    dialog.close();
    void app.importRecipe(f);
    recipeInput.value = '';
  }
</script>

<dialog bind:this={dialog} aria-labelledby="export-title">
  <div class="head">
    <h2 id="export-title">Export</h2>
    <button class="icon" onclick={() => dialog.close()} aria-label="Close"><X size={18} strokeWidth={2.5} /></button>
  </div>

  <div class="body">
    <fieldset>
      <legend>Image</legend>
      {#each formats as f (f.id)}
        <label class="opt">
          <input type="radio" name="format" value={f.id} bind:group={format} />
          <span><strong>{f.label}</strong><br /><span class="dim">{f.note}</span></span>
        </label>
      {/each}
      <div class="buttons">
        <button class="primary" onclick={() => run(() => app.download(format))} disabled={!app.doc}>
          <Download size={18} strokeWidth={2.5} /> Download
        </button>
        {#if app.doc?.path}
          <button onclick={() => run(() => app.saveOnServer(format, withRecipe))}>
            <Save size={18} strokeWidth={2.25} /> Save next to original
          </button>
        {/if}
      </div>
      {#if app.doc?.path}
        <label class="opt small">
          <input type="checkbox" bind:checked={withRecipe} />
          <span>Also save the recipe there (.recipe.toml)</span>
        </label>
        <p class="dim small">Saved as {app.doc.name.replace(/\.[^.]*$/, '')}-glitch, -glitch-1, ... Nothing is overwritten.</p>
      {/if}
    </fieldset>

    <fieldset>
      <legend>Recipe</legend>
      <p class="dim small">
        The recipe lists every step, parameter and seed. Replay it with
        <code>glitchrx --recipe FILE photo.jpg</code> or load it here.
      </p>
      <div class="buttons">
        <button onclick={() => run(() => app.downloadRecipe())} disabled={app.recipe.steps.length === 0}>Download recipe</button>
        <button onclick={() => recipeInput.click()}>Load recipe</button>
        <input bind:this={recipeInput} type="file" accept=".toml,text/plain,application/toml" class="visually-hidden" onchange={loadRecipe} tabindex="-1" />
      </div>
    </fieldset>
  </div>
</dialog>

<style>
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px var(--pad);
    border-bottom: var(--border);
  }

  .body {
    padding: var(--pad);
    display: flex;
    flex-direction: column;
    gap: var(--pad);
    overflow-y: auto;
    max-height: calc(100dvh - 120px);
  }

  fieldset {
    border: var(--border);
    margin: 0;
    padding: 10px var(--pad) var(--pad);
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  legend {
    font-family: var(--font-head);
    font-size: 1.1rem;
    padding: 0 6px;
  }

  .opt {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    cursor: pointer;
  }

  .opt input {
    margin-top: 3px;
    flex: none;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .small {
    font-size: 0.85em;
    margin: 0;
  }
</style>
