<script lang="ts">
  // Title, open, undo/redo, export, theme toggle.
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Download from '@lucide/svelte/icons/download';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import Sun from '@lucide/svelte/icons/sun';
  import Moon from '@lucide/svelte/icons/moon';
  import { app } from '../lib/app.svelte';
  import { currentTheme, toggleTheme, type Theme } from '../lib/theme';
  import { onMount } from 'svelte';

  let { onopen, onexport }: { onopen: () => void; onexport: () => void } = $props();
  let theme = $state<Theme>('dark');
  onMount(() => (theme = currentTheme()));
</script>

<header>
  <h1>glitchrx</h1>
  <div class="actions">
    <button onclick={onopen} title="Open a photo">
      <FolderOpen size={18} strokeWidth={2.25} />
      <span class="label">Open</span>
    </button>
    <button class="icon" onclick={() => app.undo()} disabled={!app.canUndo} title="Undo (Ctrl+Z)" aria-label="Undo">
      <Undo2 size={18} strokeWidth={2.25} />
    </button>
    <button class="icon" onclick={() => app.redo()} disabled={!app.canRedo} title="Redo (Ctrl+Shift+Z)" aria-label="Redo">
      <Redo2 size={18} strokeWidth={2.25} />
    </button>
    <button class="primary" onclick={onexport} disabled={!app.doc} title="Export">
      <Download size={18} strokeWidth={2.5} />
      <span class="label">Export</span>
    </button>
    <button
      class="icon"
      onclick={() => (theme = toggleTheme())}
      title="Switch to {theme === 'dark' ? 'light' : 'dark'} theme"
      aria-label="Toggle theme"
    >
      {#if theme === 'dark'}<Sun size={18} strokeWidth={2.25} />{:else}<Moon size={18} strokeWidth={2.25} />{/if}
    </button>
  </div>
</header>

<style>
  header {
    grid-area: top;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--pad);
    padding: 8px var(--pad);
    padding-top: max(8px, env(safe-area-inset-top));
    border-bottom: var(--border);
  }

  h1 {
    font-size: 1.6rem;
    letter-spacing: 0.01em;
  }

  .actions {
    display: flex;
    gap: 6px;
  }

  @media (max-width: 420px) {
    .label {
      display: none;
    }
    h1 {
      font-size: 1.3rem;
    }
  }
</style>
