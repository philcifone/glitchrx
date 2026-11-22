<script lang="ts">
  // Title, open, presets, undo/redo, export, theme toggle.
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Bookmark from '@lucide/svelte/icons/bookmark';
  import Download from '@lucide/svelte/icons/download';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import Sun from '@lucide/svelte/icons/sun';
  import Moon from '@lucide/svelte/icons/moon';
  import { app } from '../lib/app.svelte';
  import { currentTheme, toggleTheme, type Theme } from '../lib/theme';
  import { onMount } from 'svelte';

  let { onopen, onexport, onpresets }: { onopen: () => void; onexport: () => void; onpresets: () => void } = $props();
  let theme = $state<Theme>('dark');
  onMount(() => (theme = currentTheme()));
</script>

<header>
  <h1>glitchrx</h1>
  <div class="actions">
    <button class="plain" onclick={onopen} title="Open a photo">
      <FolderOpen size={18} strokeWidth={1.75} />
      <span class="label">Open</span>
    </button>
    {#if app.config?.presets}
      <button class="icon" onclick={onpresets} title="Presets: your saved recipes" aria-label="Presets">
        <Bookmark size={18} strokeWidth={1.75} />
      </button>
    {/if}
    <button class="icon" onclick={() => app.undo()} disabled={!app.canUndo} title="Undo (Ctrl+Z)" aria-label="Undo">
      <Undo2 size={18} strokeWidth={1.75} />
    </button>
    <button class="icon" onclick={() => app.redo()} disabled={!app.canRedo} title="Redo (Ctrl+Shift+Z)" aria-label="Redo">
      <Redo2 size={18} strokeWidth={1.75} />
    </button>
    <button class="primary" onclick={onexport} disabled={!app.doc} title="Export">
      <Download size={18} strokeWidth={2} />
      <span class="label">Export</span>
    </button>
    <button
      class="icon"
      onclick={() => (theme = toggleTheme())}
      title="Switch to {theme === 'dark' ? 'light' : 'dark'} theme"
      aria-label="Toggle theme"
    >
      {#if theme === 'dark'}<Sun size={18} strokeWidth={1.75} />{:else}<Moon size={18} strokeWidth={1.75} />{/if}
    </button>
  </div>
</header>

<style>
  /* A translucent navigation bar, as on iOS. */
  header {
    grid-area: top;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--pad);
    padding: 6px 12px 6px var(--pad);
    padding-top: max(6px, env(safe-area-inset-top));
    border-bottom: var(--hairline);
    background: var(--bar);
    backdrop-filter: saturate(180%) blur(20px);
    -webkit-backdrop-filter: saturate(180%) blur(20px);
    position: relative;
    z-index: 2;
  }

  h1 {
    font-size: 1.3rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .actions .primary {
    margin: 0 6px;
    border-radius: 999px;
    padding: 6px 16px;
  }

  @media (max-width: 420px) {
    .label {
      display: none;
    }
    .actions .primary {
      padding: 6px 12px;
    }
  }
</style>
