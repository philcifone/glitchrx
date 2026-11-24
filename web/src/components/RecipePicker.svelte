<script lang="ts">
  // Load a recipe saved on the server (SPEC 5e): every `.recipe.toml` in
  // the library and export folders, newest first, with those made on the
  // open photo at the top. No file picker, which on iOS cannot see the
  // server's folders anyway.
  import FileText from '@lucide/svelte/icons/file-text';
  import * as api from '../lib/api';
  import { app } from '../lib/app.svelte';
  import { ago, recipeTitle } from '../lib/age';
  import type { SavedRecipe } from '../lib/types';

  let dialog: HTMLDialogElement;
  let all = $state<SavedRecipe[] | null>(null);
  let error = $state<string | null>(null);

  export async function open() {
    all = null;
    error = null;
    dialog.showModal();
    try {
      all = await api.listRecipes();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  const key = (r: SavedRecipe) => `${r.place}:${r.path}`;
  const mine = $derived(new Set((app.doc?.recipes ?? []).map(key)));
  const sections = $derived.by(() => {
    if (!all) return [];
    const here = all.filter((r) => mine.has(key(r)));
    const rest = all.filter((r) => !mine.has(key(r)));
    return [
      { title: 'Made on this photo', list: here },
      { title: here.length ? 'Other recipes' : 'Recipes', list: rest },
    ].filter((s) => s.list.length);
  });

  async function choose(r: SavedRecipe) {
    dialog.close();
    await app.loadSaved(r);
  }

  /** Where it is: the folder's name, and any subfolder. */
  function where(r: SavedRecipe): string {
    const dir = r.path.includes('/') ? r.path.slice(0, r.path.lastIndexOf('/')) : '';
    const base = r.place === 'export' ? 'Export folder' : 'Library';
    return dir ? `${base}/${dir}` : base;
  }
</script>

<dialog bind:this={dialog} aria-labelledby="recipes-title">
  <div class="head">
    <span></span>
    <h2 id="recipes-title">Saved recipes</h2>
    <button class="plain" onclick={() => dialog.close()}>Cancel</button>
  </div>
  <div class="body">
    {#if error}
      <p class="err">{error}</p>
    {:else if !all}
      <p class="dim small">Looking in the library and export folders...</p>
    {:else if !all.length}
      <p class="dim small">
        No recipes in the library or export folder yet. Turn on "Also save the recipe beside it" when saving, and they show up here.
      </p>
    {:else}
      {#each sections as s (s.title)}
        <h3 class="section-label">{s.title}</h3>
        <ul class="list">
          {#each s.list as r (key(r))}
            <li>
              <button onclick={() => choose(r)}>
                <FileText size={18} strokeWidth={1.75} />
                <span class="text">
                  <span class="name">{recipeTitle(r.name)}</span>
                  <span class="dim meta">
                    {where(r)}, {ago(r.modified)}{r.source && !mine.has(key(r)) ? `, made on ${r.source}` : ''}
                  </span>
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/each}
      <p class="dim small">Loading replaces the steps; undo brings them back.</p>
    {/if}
  </div>
</dialog>

<style>
  .head {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    gap: 8px;
    padding: 10px;
    border-bottom: var(--hairline);
  }

  .head .plain {
    justify-self: end;
  }

  .head h2 {
    font-size: 1.1rem;
    text-align: center;
  }

  .body {
    padding: var(--pad);
    overflow-y: auto;
    max-height: calc(100dvh - 120px);
  }

  .section-label {
    margin: 14px 4px 6px;
  }

  .section-label:first-child {
    margin-top: 0;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    background: var(--card);
    border-radius: 12px;
    overflow: hidden;
  }

  .list li + li {
    border-top: var(--hairline);
  }

  .list button {
    width: 100%;
    justify-content: flex-start;
    gap: 12px;
    padding: 10px 14px;
    background: none;
    border-radius: 0;
    text-align: left;
    font-weight: 400;
  }

  .list button :global(svg) {
    color: var(--accent);
    flex: none;
  }

  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name,
  .meta {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    font-size: 0.78rem;
  }

  .small {
    font-size: 0.85rem;
    margin: 10px 4px;
  }

  .err {
    color: var(--danger, #cc241d);
  }
</style>
