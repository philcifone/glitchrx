<script lang="ts">
  // Export: download (or save on the server) at full resolution, and
  // save or load the recipe.
  import Download from '@lucide/svelte/icons/download';
  import Save from '@lucide/svelte/icons/save';
  import { app } from '../lib/app.svelte';
  import type { ExportFormat } from '../lib/types';

  let dialog: HTMLDialogElement;
  let recipeInput: HTMLInputElement;
  let format = $state<ExportFormat>('tiff');
  let withRecipe = $state(true);

  export function show(): void {
    dialog.showModal();
  }

  const formats: { id: ExportFormat; label: string; note: string }[] = [
    {
      id: 'tiff',
      label: 'TIFF',
      note: 'Uncompressed 8-bit RGB with the colour profile: ready for masking in Photoshop. Large.',
    },
    {
      id: 'png',
      label: 'PNG',
      note: 'Lossless and smaller than TIFF. Every pixel exactly as bent.',
    },
    {
      id: 'jpeg',
      label: 'JPEG',
      note: 'Quality 95. Smallest, good for sharing; fine grain softens a little.',
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
    <button class="plain" onclick={() => dialog.close()}>Done</button>
  </div>

  <div class="body">
    <h3 class="section-label">Image</h3>
    <section class="group">
      <div class="segmented" role="radiogroup" aria-label="Format">
        {#each formats as f (f.id)}
          <button role="radio" aria-checked={format === f.id} class:on={format === f.id} onclick={() => (format = f.id)}>
            {f.label}
          </button>
        {/each}
      </div>
      <p class="dim small">{formats.find((f) => f.id === format)?.note}</p>
      <div class="buttons">
        {#if app.desktop}
          <button class="primary" onclick={() => run(() => app.saveAs(format, withRecipe))} disabled={!app.doc}>
            <Save size={18} strokeWidth={2} /> Save as...
          </button>
        {:else}
          <button class="primary" onclick={() => run(() => app.download(format))} disabled={!app.doc}>
            <Download size={18} strokeWidth={2} /> Download
          </button>
        {/if}
        {#if app.doc?.path && !app.desktop}
          <button onclick={() => run(() => app.saveOnServer(format, withRecipe))}>
            <Save size={18} strokeWidth={1.75} /> Save next to original
          </button>
        {/if}
      </div>
      {#if app.desktop}
        <label class="opt">
          <span>Also save the recipe beside it</span>
          <input class="switch" type="checkbox" bind:checked={withRecipe} />
        </label>
        <p class="dim small">Suggests the next free -glitch name next to the original. The original is never written.</p>
      {:else if app.doc?.path}
        <label class="opt">
          <span>Also save the recipe there</span>
          <input class="switch" type="checkbox" bind:checked={withRecipe} />
        </label>
        <p class="dim small">Saved as {app.doc.name.replace(/\.[^.]*$/, '')}-glitch, -glitch-1, ... Nothing is overwritten.</p>
      {/if}
    </section>

    <h3 class="section-label">Recipe</h3>
    <section class="group">
      <p class="dim small">
        Every step, parameter and seed. Replay it with
        <code>glitchrx --recipe FILE photo.jpg</code> or load it here.
      </p>
      <div class="buttons">
        {#if !app.desktop}
          <button onclick={() => run(() => app.downloadRecipe())} disabled={app.recipe.steps.length === 0}>Download recipe</button>
        {/if}
        <button onclick={() => recipeInput.click()}>Load recipe</button>
        <input bind:this={recipeInput} type="file" accept=".toml,text/plain,application/toml" class="visually-hidden" onchange={loadRecipe} tabindex="-1" />
      </div>
    </section>
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
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .opt {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    border-top: var(--hairline);
    padding-top: 12px;
    cursor: pointer;
  }

  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .small {
    font-size: 0.85rem;
    margin: 0;
  }
</style>
