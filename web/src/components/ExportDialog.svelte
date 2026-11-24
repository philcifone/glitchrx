<script lang="ts">
  // Export: download (or save on the server) at full resolution, and
  // save or load the recipe. The format and its settings are remembered
  // on this device. Save As writes into a folder on the server, under its
  // home folder, with a name of your choosing (SPEC 4c, 4d).
  //
  // Or a project (.glrx, D-63) instead of the picture: the original, the
  // steps and a preview in one file, to open again on any device. A photo
  // opened from a project can be saved back over it.
  import Download from '@lucide/svelte/icons/download';
  import Save from '@lucide/svelte/icons/save';
  import Folder from '@lucide/svelte/icons/folder';
  import { app } from '../lib/app.svelte';
  import * as api from '../lib/api';
  import { WEBP_MAX_SIDE, cleanPrefs, isLossy, optionsOf, type ExportPrefs } from '../lib/exportprefs';
  import { storageGet, storageSet } from '../lib/storage';
  import type { ExportFormat } from '../lib/types';
  import RecipePicker from './RecipePicker.svelte';
  import FolderPicker from './FolderPicker.svelte';

  const PREFS_KEY = 'glitchrx.export';

  let dialog: HTMLDialogElement;
  let recipes: RecipePicker;
  let recipeInput: HTMLInputElement;
  let picker: FolderPicker;
  let prefs = $state<ExportPrefs>(load());
  let withRecipe = $state(true);
  /** Save As: the folder (relative to home) and the file name. */
  let folder = $state<string | null>(null);
  let name = $state('');

  function load(): ExportPrefs {
    try {
      return cleanPrefs(JSON.parse(storageGet(PREFS_KEY) ?? 'null'));
    } catch {
      return cleanPrefs(null);
    }
  }

  function set(patch: Partial<ExportPrefs>) {
    prefs = { ...prefs, ...patch };
    storageSet(PREFS_KEY, JSON.stringify(prefs));
    if (patch.format || patch.kind) void suggest();
  }

  /** Save As is available: a browser on a server with settings. */
  const saveAsHere = $derived(!app.desktop && !!app.config?.settings);

  export async function show(): Promise<void> {
    dialog.showModal();
    if (saveAsHere) {
      await app.loadSettings();
      folder ??= app.folderSettings?.export ?? app.folderSettings?.library ?? '';
      void suggest();
    }
  }

  /** The next free -glitch name in the folder. */
  async function suggest() {
    const doc = app.doc;
    if (!doc || folder === null || !saveAsHere) return;
    try {
      name = await api.suggestName(doc.id, folder, prefs.format, project);
    } catch {
      // The folder may have gone; leave the name as it was.
    }
  }

  async function changeFolder() {
    const f = await picker.pick('Save in', folder);
    if (f === null) return;
    folder = f;
    void suggest();
  }

  const FORMATS: { id: ExportFormat; label: string; note: string }[] = [
    { id: 'tiff', label: 'TIFF', note: 'Uncompressed 8-bit RGB with the colour profile: ready for masking in Photoshop. Large.' },
    { id: 'png', label: 'PNG', note: 'Lossless and smaller than TIFF. Every pixel exactly as bent.' },
    { id: 'jpeg', label: 'JPEG', note: 'Small and readable everywhere.' },
    { id: 'webp', label: 'WebP', note: 'Smaller than JPEG at the same look, or lossless and smaller than PNG.' },
    { id: 'avif', label: 'AVIF', note: 'The smallest files. Slow to encode: about 10 seconds for a 20 MP photo at speed 6.' },
  ];

  const doc = $derived(app.doc);
  const project = $derived(prefs.kind === 'project');
  const opts = $derived(optionsOf(prefs));
  /** The extension Save As adds. */
  const ext = $derived(project ? 'glrx' : prefs.format === 'jpeg' ? 'jpg' : prefs.format === 'tiff' ? 'tif' : prefs.format);
  /** Recipe files go beside pictures; a project holds its own. */
  const recipeBeside = $derived(withRecipe && !project);

  /** Save over the project the photo was opened from, once asked. */
  function replaceProject() {
    if (!doc?.project || !confirm(`Replace ${doc.project} with these steps?`)) return;
    void run(() => app.saveOnServer({ ...opts, replace: true }, false));
  }
  const out = $derived(app.outputSize);
  const tooBigForWebp = $derived(!!out && Math.max(out.w, out.h) > WEBP_MAX_SIDE);
  const canvas = $derived(app.canvasSize);
  const cropped = $derived(app.croppedSize);
  // Shrunk to a work size: the crop (or photo) and the canvas differ.
  const shrunk = $derived(!!cropped && !!canvas && canvas.w !== cropped.w);
  const blocked = $derived(!project && prefs.format === 'webp' && tooBigForWebp);

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

{#snippet slider(label: string, value: number, min: number, max: number, onset: (v: number) => void, hint = '')}
  <label class="slider">
    <span class="top"><span>{label}</span><span class="val">{value}</span></span>
    <input
      type="range"
      {min}
      {max}
      step="1"
      {value}
      style:--pct="{((value - min) / (max - min)) * 100}%"
      oninput={(e) => onset(Number(e.currentTarget.value))}
      aria-label={label}
    />
    {#if hint}<span class="dim tiny">{hint}</span>{/if}
  </label>
{/snippet}

<dialog bind:this={dialog} aria-labelledby="export-title">
  <div class="head">
    <h2 id="export-title">Export</h2>
    <button class="plain" onclick={() => dialog.close()}>Done</button>
  </div>

  <div class="body">
    <div class="segmented kind" role="radiogroup" aria-label="What to save">
      <button role="radio" aria-checked={!project} class:on={!project} onclick={() => set({ kind: 'picture' })}>Picture</button>
      <button role="radio" aria-checked={project} class:on={project} onclick={() => set({ kind: 'project' })}>Project</button>
    </div>

    {#if project}
    <section class="group">
      <p class="small">
        The original photo, every step and a preview, in one .glrx file: open it again on any device and carry on, as with a .psd.
      </p>
      <p class="dim small">The photo goes in as it is, except for where it was taken, which is left out.</p>
      <div class="buttons">
        {#if app.desktop}
          <button class="primary" onclick={() => run(() => app.saveAs(opts, false))} disabled={!doc}>
            <Save size={18} strokeWidth={2} /> Save as...
          </button>
        {:else}
          <button class="primary" onclick={() => run(() => app.download(opts))} disabled={!doc}>
            <Download size={18} strokeWidth={2} /> Download
          </button>
        {/if}
        {#if doc?.path && !app.desktop}
          {#if doc.project}
            <button onclick={replaceProject}>
              <Save size={18} strokeWidth={1.75} /> Save over {doc.project}
            </button>
            <button onclick={() => run(() => app.saveOnServer(opts, false))}>Save as new</button>
          {:else}
            <button onclick={() => run(() => app.saveOnServer(opts, false))}>
              <Save size={18} strokeWidth={1.75} /> Save next to original
            </button>
          {/if}
        {/if}
      </div>
    </section>
    {:else}
    <section class="group">
      <div class="segmented" role="radiogroup" aria-label="Format">
        {#each FORMATS as f (f.id)}
          <button role="radio" aria-checked={prefs.format === f.id} class:on={prefs.format === f.id} onclick={() => set({ format: f.id })}>
            {f.label}
          </button>
        {/each}
      </div>
      <p class="dim small">{FORMATS.find((f) => f.id === prefs.format)?.note}</p>

      {#if prefs.format === 'jpeg'}
        {@render slider('Quality', prefs.jpegQuality, 1, 100, (v) => set({ jpegQuality: v }))}
      {:else if prefs.format === 'webp'}
        <div class="segmented" role="radiogroup" aria-label="WebP compression">
          <button role="radio" aria-checked={!prefs.webpLossless} class:on={!prefs.webpLossless} onclick={() => set({ webpLossless: false })}>Lossy</button>
          <button role="radio" aria-checked={prefs.webpLossless} class:on={prefs.webpLossless} onclick={() => set({ webpLossless: true })}>Lossless</button>
        </div>
        {#if !prefs.webpLossless}
          {@render slider('Quality', prefs.webpQuality, 1, 100, (v) => set({ webpQuality: v }))}
        {/if}
        {#if tooBigForWebp}
          <p class="warn small">WebP holds at most {WEBP_MAX_SIDE} pixels a side; this export is {out?.w}x{out?.h}.</p>
        {/if}
      {:else if prefs.format === 'avif'}
        <div class="two">
          {@render slider('Quality', prefs.avifQuality, 1, 100, (v) => set({ avifQuality: v }))}
          {@render slider('Speed', prefs.avifSpeed, 1, 10, (v) => set({ avifSpeed: v }), '1 smallest, 10 fastest')}
        </div>
        {#if doc && !doc.avif_keeps_profile}
          <p class="warn small">
            AVIF cannot carry this photo's colour profile{doc.profile ? ` (${doc.profile})` : ''}, so colours may shift. TIFF, PNG, JPEG and WebP keep it.
          </p>
        {/if}
      {/if}
      {#if shrunk && cropped && canvas}
        <div class="segmented" role="radiogroup" aria-label="Export size">
          <button role="radio" aria-checked={!app.recipe.scale_back} class:on={!app.recipe.scale_back} onclick={() => app.setScaleBack(false)}>
            {canvas.w}x{canvas.h}
          </button>
          <button role="radio" aria-checked={!!app.recipe.scale_back} class:on={!!app.recipe.scale_back} onclick={() => app.setScaleBack(true)}>
            {cropped.w}x{cropped.h}
          </button>
        </div>
        <p class="dim small">
          The steps run at the work size. Export it at that size, or enlarged back to the photo's with hard pixel edges, so the coarse texture stays crisp. Saved in the recipe.
        </p>
      {/if}
      {#if isLossy(prefs)}
        <p class="dim small">Lossy: fine glitch texture (single-pixel noise, hard stripes) is smoothed a little. TIFF, PNG and lossless WebP keep every pixel.</p>
      {/if}

      <div class="buttons">
        {#if app.desktop}
          <button class="primary" onclick={() => run(() => app.saveAs(opts, withRecipe))} disabled={!doc || blocked}>
            <Save size={18} strokeWidth={2} /> Save as...
          </button>
        {:else}
          <button class="primary" onclick={() => run(() => app.download(opts))} disabled={!doc || blocked}>
            <Download size={18} strokeWidth={2} /> Download
          </button>
        {/if}
        {#if doc?.path && !app.desktop}
          <button onclick={() => run(() => app.saveOnServer(opts, withRecipe))} disabled={blocked}>
            <Save size={18} strokeWidth={1.75} /> Save next to original
          </button>
        {/if}
      </div>
      {#if app.desktop}
        <p class="dim small">Suggests the next free -glitch name next to the original. The original is never written.</p>
      {:else if doc?.path}
        <p class="dim small">Next to the original as {doc.name.replace(/\.[^.]*$/, '')}-glitch, -glitch-1, ... Nothing is overwritten.</p>
      {/if}
    </section>
    {/if}

    {#if saveAsHere}
      <h3 class="section-label">Save as</h3>
      <section class="group">
        <div class="row">
          <span class="lbl">Folder</span>
          <button class="plain folder" onclick={changeFolder} title="Choose the folder">
            <Folder size={16} strokeWidth={1.75} />
            <span class="path">~/{folder ?? ''}</span>
          </button>
        </div>
        <label class="row">
          <span class="lbl">Name</span>
          <input type="text" bind:value={name} autocapitalize="off" autocomplete="off" spellcheck="false" aria-label="File name" />
          <span class="dim ext">.{ext}</span>
        </label>
        <div class="buttons">
          <button
            class="primary"
            onclick={() => run(() => app.saveTo(opts, folder ?? '', name.trim(), recipeBeside))}
            disabled={!doc || blocked || !name.trim() || folder === null}
          >
            <Save size={18} strokeWidth={2} /> Save
          </button>
        </div>
        <p class="dim small">On the server, in its home folder. An existing file is never replaced. The default folder is the export folder in Settings.</p>
      </section>
    {/if}

    {#if !project && (app.desktop || doc?.path || saveAsHere)}
      <label class="opt">
        <span>Also save the recipe beside it</span>
        <input class="switch" type="checkbox" bind:checked={withRecipe} />
      </label>
    {/if}

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
        <button onclick={() => recipes.open()}>Saved on the server</button>
        <!-- No accept filter: iOS has no file type for .toml and would grey the
             recipe out. The server checks what is picked. -->
        <input bind:this={recipeInput} type="file" class="visually-hidden" onchange={loadRecipe} tabindex="-1" />
      </div>
    </section>
  </div>
</dialog>

<FolderPicker bind:this={picker} />
<RecipePicker bind:this={recipes} />

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

  .kind {
    align-self: stretch;
    margin-top: 6px;
  }

  .kind button {
    flex: 1;
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
    background: var(--card);
    border-radius: 12px;
    padding: 12px 14px;
    cursor: pointer;
  }

  .slider {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .slider .top {
    display: flex;
    justify-content: space-between;
    font-size: 0.9rem;
  }

  .val {
    color: var(--fg-dim);
    font-variant-numeric: tabular-nums;
  }

  .tiny {
    font-size: 0.75rem;
  }

  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .lbl {
    flex: none;
    width: 4em;
    font-size: 0.9rem;
    font-weight: 500;
  }

  .row input[type='text'] {
    flex: 1;
    min-width: 0;
  }

  .ext {
    flex: none;
    font-size: 0.9rem;
  }

  .folder {
    flex: 1;
    min-width: 0;
    justify-content: flex-start;
    gap: 8px;
    color: var(--accent);
  }

  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .warn {
    color: var(--danger);
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
