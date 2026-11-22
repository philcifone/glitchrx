<script lang="ts">
  // Open a photo: upload from this device, or browse the server's --root.
  import Upload from '@lucide/svelte/icons/upload';
  import X from '@lucide/svelte/icons/x';
  import { app } from '../lib/app.svelte';
  import * as api from '../lib/api';
  import type { Listing } from '../lib/types';

  let dialog: HTMLDialogElement;
  let fileInput: HTMLInputElement;
  let listing = $state<Listing | null>(null);
  let browseError = $state<string | null>(null);

  export function show(): void {
    dialog.showModal();
    if (app.config?.browse && !listing) void go('');
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
    <button class="icon" onclick={() => dialog.close()} aria-label="Close"><X size={18} strokeWidth={2.5} /></button>
  </div>

  <div class="body">
    <section>
      <input bind:this={fileInput} type="file" accept="image/jpeg,image/png,image/tiff,.jpg,.jpeg,.png,.tif,.tiff" class="visually-hidden" onchange={pickFile} tabindex="-1" />
      <button class="primary wide" onclick={() => fileInput.click()}>
        <Upload size={18} strokeWidth={2.5} /> From this device
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
          {#if listing.dirs.length}
            <ul class="dirs">
              {#if listing.parent !== null}
                <li><button onclick={() => go(listing!.parent ?? '')}>..</button></li>
              {/if}
              {#each listing.dirs as d (d.path)}
                <li><button onclick={() => go(d.path)}>{d.name}/</button></li>
              {/each}
            </ul>
          {:else if listing.parent !== null}
            <ul class="dirs"><li><button onclick={() => go(listing!.parent ?? '')}>..</button></li></ul>
          {/if}
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
      <p class="dim small">Start the server with <code>--root DIR</code> to open and save photos on the server.</p>
    {/if}
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

  .wide {
    width: 100%;
  }

  .small {
    font-size: 0.85em;
    margin: 6px 0 0;
  }

  .browse {
    border-top: var(--border);
    padding-top: var(--pad);
  }

  .crumbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-bottom: 8px;
  }

  .link {
    border: none;
    background: none;
    padding: 2px 4px;
    min-height: 30px;
    color: var(--accent);
    text-decoration: underline;
  }

  ul {
    list-style: none;
    margin: 0 0 var(--pad);
    padding: 0;
  }

  .dirs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .files {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 8px;
  }

  .files button {
    width: 100%;
    flex-direction: column;
    padding: 4px;
    gap: 4px;
  }

  .files img {
    width: 100%;
    aspect-ratio: 4 / 3;
    object-fit: cover;
    background: var(--bg2);
    display: block;
  }

  .files span {
    font-size: 0.8em;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .err {
    color: var(--danger);
  }
</style>
