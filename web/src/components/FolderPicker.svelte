<script lang="ts">
  // Pick a folder on the server, inside its home folder (SPEC 4c): for the
  // library, the export folder and Save As. Folders only. `pick` resolves
  // with the chosen folder (relative to home, "" for home itself), or null
  // if cancelled.
  //
  // New folder (D-65) makes one inside the folder shown and opens it; only
  // when asked, the app never makes folders by itself.
  import FolderPlus from '@lucide/svelte/icons/folder-plus';
  import DirList from './DirList.svelte';
  import * as api from '../lib/api';
  import type { FolderListing } from '../lib/types';

  let dialog: HTMLDialogElement;
  let title = $state('Choose a folder');
  let listing = $state<FolderListing | null>(null);
  let error = $state<string | null>(null);
  let resolve: ((path: string | null) => void) | null = null;
  /** The New folder row: its name while open, null when closed. */
  let newName = $state<string | null>(null);
  let newError = $state<string | null>(null);
  let nameInput = $state<HTMLInputElement>();

  /** Show the picker at `start` (or home). */
  export function pick(heading: string, start: string | null): Promise<string | null> {
    title = heading;
    listing = null;
    dialog.showModal();
    void go(start ?? '', true);
    return new Promise((r) => (resolve = r));
  }

  async function go(path: string, fallBack = false) {
    newName = null;
    try {
      listing = await api.folders(path);
      error = null;
    } catch (e) {
      // A remembered folder that has gone: start from home instead.
      if (fallBack && path) return go('');
      error = e instanceof Error ? e.message : String(e);
    }
  }

  function startNew() {
    newName = '';
    newError = null;
    // After the row renders.
    queueMicrotask(() => nameInput?.focus());
  }

  async function makeFolder(e: SubmitEvent) {
    e.preventDefault();
    if (!listing || !newName?.trim()) return;
    try {
      listing = await api.newFolder(listing.path, newName.trim());
      newName = null;
    } catch (err) {
      newError = err instanceof Error ? err.message : String(err);
    }
  }

  function finish(path: string | null) {
    resolve?.(path);
    resolve = null;
    dialog.close();
  }

  const crumbs = $derived.by(() => {
    if (!listing) return [];
    const parts = listing.path ? listing.path.split('/') : [];
    return parts.map((name, i) => ({ name, path: parts.slice(0, i + 1).join('/') }));
  });
</script>

<dialog bind:this={dialog} aria-labelledby="folder-title" onclose={() => finish(null)}>
  <div class="head">
    <button class="plain" onclick={() => finish(null)}>Cancel</button>
    <h2 id="folder-title">{title}</h2>
    <button class="plain strong" onclick={() => finish(listing?.path ?? null)} disabled={!listing}>Choose</button>
  </div>
  <div class="body">
    <nav class="crumbs" aria-label="Folder">
      <button class="link" onclick={() => go('')}>Home</button>
      {#each crumbs as c (c.path)}
        <span class="dim">/</span>
        <button class="link" onclick={() => go(c.path)}>{c.name}</button>
      {/each}
    </nav>
    {#if error}
      <p class="err">{error}</p>
    {:else if listing}
      <DirList dirs={listing.dirs} parent={listing.parent} ongo={go} />
      {#if !listing.dirs.length}<p class="dim small">No folders inside this one.</p>{/if}
      {#if newName === null}
        <button class="link new" onclick={startNew}>
          <FolderPlus size={18} strokeWidth={1.75} /> New folder
        </button>
      {:else}
        <form class="new-row" onsubmit={makeFolder}>
          <input
            bind:this={nameInput}
            bind:value={newName}
            placeholder="Folder name"
            aria-label="New folder name"
            autocapitalize="off"
            autocomplete="off"
            spellcheck="false"
            onkeydown={(e) => e.key === 'Escape' && (e.stopPropagation(), e.preventDefault(), (newName = null))}
          />
          <button type="button" class="plain" onclick={() => (newName = null)}>Cancel</button>
          <button type="submit" class="primary" disabled={!newName.trim()}>Make</button>
        </form>
        {#if newError}<p class="err small">{newError}</p>{/if}
      {/if}
      <p class="dim small">Choose takes the folder you are in: ~/{listing.path}</p>
    {/if}
  </div>
</dialog>

<style>
  .head {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 10px;
    border-bottom: var(--hairline);
  }

  .head h2 {
    font-size: 1.1rem;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .strong {
    font-weight: 600;
  }

  .body {
    padding: var(--pad);
    overflow-y: auto;
    max-height: calc(100dvh - 120px);
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

  .small {
    font-size: 0.85rem;
    margin: 10px 0 0;
  }

  .new {
    margin-top: 6px;
    gap: 6px;
  }

  .new-row {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }

  .new-row input {
    flex: 1;
    min-width: 0;
  }

  .err {
    color: var(--danger);
  }
</style>
