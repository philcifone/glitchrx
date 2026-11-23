<script lang="ts">
  // Settings (SPEC 4c): how the app looks, what Open browses and where
  // Save As starts (both folders on the server, shared by every device
  // using it), and what opening another photo does with the steps (this
  // device only).
  import Folder from '@lucide/svelte/icons/folder';
  import { app, type NewPhotoChoice } from '../lib/app.svelte';
  import { setTheme, themeChoice, type ThemeChoice } from '../lib/theme';
  import FolderPicker from './FolderPicker.svelte';

  let dialog: HTMLDialogElement;
  let picker: FolderPicker;
  let theme = $state<ThemeChoice>('system');

  export function show(): void {
    theme = themeChoice();
    void app.loadSettings();
    dialog.showModal();
  }

  const THEMES: { id: ThemeChoice; label: string }[] = [
    { id: 'light', label: 'Light' },
    { id: 'dark', label: 'Dark' },
    { id: 'system', label: 'System' },
  ];

  const NEW_PHOTO: { id: NewPhotoChoice; label: string }[] = [
    { id: 'ask', label: 'Ask' },
    { id: 'keep', label: 'Keep steps' },
    { id: 'fresh', label: 'Start fresh' },
  ];

  function chooseTheme(t: ThemeChoice) {
    theme = t;
    setTheme(t);
  }

  async function change(which: 'library' | 'export') {
    const current = app.folderSettings?.[which] ?? null;
    const picked = await picker.pick(which === 'library' ? 'Library folder' : 'Export folder', current);
    if (picked !== null) await app.setFolder(which, picked);
  }

  const shown = (p: string | null | undefined) => (p === null || p === undefined ? 'Not chosen' : `~/${p}`);
  const fs = $derived(app.folderSettings);
</script>

<dialog bind:this={dialog} aria-labelledby="settings-title">
  <div class="head">
    <h2 id="settings-title">Settings</h2>
    <button class="plain" onclick={() => dialog.close()}>Done</button>
  </div>

  <div class="body">
    <h3 class="section-label">Appearance</h3>
    <div class="segmented" role="radiogroup" aria-label="Appearance">
      {#each THEMES as t (t.id)}
        <button role="radio" aria-checked={theme === t.id} class:on={theme === t.id} onclick={() => chooseTheme(t.id)}>{t.label}</button>
      {/each}
    </div>

    {#if app.config?.settings}
      <h3 class="section-label">Folders on the server</h3>
      <section class="group">
        <div class="row">
          <span class="text">
            <span>Library</span>
            <span class="dim small">What Open browses</span>
          </span>
          {#if fs?.fixed_root}
            <span class="dim small fixed" title="Started with --root, which decides what Open browses">{fs.fixed_root} (--root)</span>
          {:else}
            <button class="plain pick" onclick={() => change('library')}>
              <Folder size={16} strokeWidth={1.75} /> <span>{shown(fs?.library)}</span>
            </button>
          {/if}
        </div>
        <div class="row">
          <span class="text">
            <span>Export</span>
            <span class="dim small">Where Save As starts</span>
          </span>
          <button class="plain pick" onclick={() => change('export')}>
            <Folder size={16} strokeWidth={1.75} /> <span>{shown(fs?.export)}</span>
          </button>
        </div>
      </section>
      <p class="dim small note">Folders inside the server's home folder, kept in ~/.config/glitchrx/config.toml for every device.</p>
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

<FolderPicker bind:this={picker} />

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
    padding: 4px 14px;
    display: flex;
    flex-direction: column;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 52px;
    min-width: 0;
  }

  .row + .row {
    border-top: var(--hairline);
  }

  .text {
    display: flex;
    flex-direction: column;
    line-height: 1.25;
    flex: none;
  }

  .pick {
    min-width: 0;
    gap: 6px;
    color: var(--accent);
  }

  .pick span,
  .fixed {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  .small {
    font-size: 0.8rem;
  }

  .note {
    margin: 0 4px;
  }
</style>
