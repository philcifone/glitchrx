<script lang="ts">
  // Open a photo: upload from this device, or browse the server's library
  // (the folder chosen in Settings, or --root).
  import Upload from '@lucide/svelte/icons/upload';
  import DirList from './DirList.svelte';
  import { app } from '../lib/app.svelte';
  import * as api from '../lib/api';
  import type { Listing } from '../lib/types';

  let dialog: HTMLDialogElement;
  let fileInput: HTMLInputElement;
  let listing = $state<Listing | null>(null);
  let browseError = $state<string | null>(null);

  /** The library the listing came from; a new one (Settings) starts over. */
  let listedRoot: string | null = null;

  export function show(): void {
    dialog.showModal();
    const root = app.config?.root_name ?? null;
    if (app.config?.browse && (!listing || listedRoot !== root)) {
      listedRoot = root;
      void go('');
    }
  }

  async function go(path: string) {
    try {
      listing = await api.browse(path);
      browseError = null;
    } catch (e) {
      browseError = e instanceof Error ? e.message : String(e);
    }
  }

  function pickFile() {
    const f = fileInput.files?.[0];
    if (!f) return;
    dialog.close();
    void app.openFile(f);
    fileInput.value = '';
  }

  function openPath(path: string) {
    dialog.close();
    void app.openPath(path);
  }

  const crumbs = $derived.by(() => {
    if (!listing) return [];
    const parts = listing.path ? listing.path.split('/') : [];
    return parts.map((name, i) => ({ name, path: parts.slice(0, i + 1).join('/') }));
  });

  const mb = (n: number) => `${(n / (1 << 20)).toFixed(1)} MB`;
</script>

<dialog bind:this={dialog} aria-labelledby="open-title">
  <div class="head">
    <h2 id="open-title">Open a photo</h2>
    <button class="plain" onclick={() => dialog.close()}>Cancel</button>
  </div>

  <div class="body">
    <section>
      <input bind:this={fileInput} type="file" accept="image/jpeg,image/png,image/tiff,.jpg,.jpeg,.png,.tif,.tiff" class="visually-hidden" onchange={pickFile} tabindex="-1" />
      <button class="primary wide" onclick={() => fileInput.click()}>
        <Upload size={18} strokeWidth={2} /> From this device
      </button>
      <p class="dim small">The photo is sent to this glitchrx server only. JPEG, PNG or TIFF.</p>
    </section>

    {#if app.config?.browse}
      <section class="browse">
        <nav class="crumbs" aria-label="Folder">
          <button class="link" onclick={() => go('')}>{app.config.root_name ?? 'root'}</button>
          {#each crumbs as c (c.path)}
            <span class="dim">/</span>
            <button class="link" onclick={() => go(c.path)}>{c.name}</button>
          {/each}
        </nav>
        {#if browseError}
          <p class="err">{browseError}</p>
        {:else if listing}
          <DirList dirs={listing.dirs} parent={listing.parent} ongo={go} />
          {#if listing.files.length}
            <ul class="files">
              {#each listing.files as f (f.path)}
                <li>
                  <button onclick={() => openPath(f.path)} title="{f.name}, {mb(f.size)}">
                    <img src={api.thumbUrl(f.path)} alt="" loading="lazy" decoding="async" />
                    <span>{f.name}</span>
                  </button>
                </li>
              {/each}
            </ul>
          {:else}
            <p class="dim">No photos in this folder.</p>
          {/if}
        {/if}
      </section>
    {:else}
      <p class="dim small">
        To open photos stored on the server, choose a library folder in Settings{app.config?.settings ? '' : ' (or start it with --root DIR)'}.
      </p>
    {/if}
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
    padding: var(--pad);
    display: flex;
    flex-direction: column;
    gap: var(--pad);
    overflow-y: auto;
    max-height: calc(100dvh - 120px);
  }

  .wide {
    width: 100%;
    min-height: 44px;
  }

  .small {
    font-size: 0.85rem;
    margin: 8px 0 0;
  }

  .browse {
    border-top: var(--hairline);
    padding-top: var(--pad);
  }

  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 2px;
    margin-bottom: 10px;
    font-size: 0.9rem;
  }

  .link {
    background: none;
    padding: 2px 6px;
    min-height: 30px;
    color: var(--accent);
  }

  ul {
    list-style: none;
    margin: 0 0 var(--pad);
    padding: 0;
  }

  .files {
    margin-top: var(--pad);
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 12px;
  }

  .files button {
    width: 100%;
    flex-direction: column;
    align-items: stretch;
    padding: 0;
    gap: 6px;
    background: none;
    font-weight: 400;
  }

  .files button:hover:not(:disabled) {
    background: none;
  }

  .files button:hover img {
    filter: brightness(0.9);
  }

  .files img {
    width: 100%;
    aspect-ratio: 4 / 3;
    object-fit: cover;
    border-radius: 10px;
    background: var(--fill);
    display: block;
  }

  .files span {
    font-size: 0.78rem;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: center;
  }

  .err {
    color: var(--danger);
  }
</style>
