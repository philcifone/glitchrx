<script lang="ts">
  // Title, open, presets, undo/redo, export, theme toggle. While a mask is
  // being painted it becomes an edit bar, as in iOS: what is being masked,
  // undo/redo (used constantly while painting) and Done. Nothing floats
  // over the photo, and Open and Export wait until painting is done.
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
  {#if app.maskTarget}
    {@const t = app.maskTarget}
    <div class="mode" role="status">
      <h1>Mask</h1>
      <span class="what">Step {t.n}: {t.step.effect}{t.step.enabled ? '' : ' (off)'}</span>
    </div>
    <div class="actions">
      <button class="icon" onclick={() => app.undo()} disabled={!app.canUndo} title="Undo the last stroke (Ctrl+Z)" aria-label="Undo">
        <Undo2 size={18} strokeWidth={1.75} />
      </button>
      <button class="icon" onclick={() => app.redo()} disabled={!app.canRedo} title="Redo (Ctrl+Shift+Z)" aria-label="Redo">
        <Redo2 size={18} strokeWidth={1.75} />
      </button>
      <button class="primary" onclick={() => app.stopMasking()} title="Finish painting (Enter)">Done</button>
    </div>
  {:else}
    <h1>glitchrx</h1>
    <div class="actions">
      <button class="plain" onclick={onopen} title="Open a photo" aria-label="Open">
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
      <button class="primary" onclick={onexport} disabled={!app.doc} title="Export" aria-label="Export">
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
  {/if}
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

  /* The title gives way first, so the buttons never leave the screen. */
  h1 {
    font-size: 1.3rem;
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mode {
    display: flex;
    align-items: baseline;
    gap: 10px;
    min-width: 0;
  }

  /* While masking, the step name gives way, never the title. */
  .mode h1 {
    flex: none;
  }

  .what {
    min-width: 0;
    color: var(--fg-dim);
    font-size: 0.95rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .actions {
    flex: none;
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .actions .primary {
    margin: 0 6px;
    border-radius: 999px;
    padding: 6px 16px;
  }

  /* Text labels only where there is room: tablets and desktops. Every
     phone, up to a Pro Max at 430 px, gets icons. */
  @media (max-width: 599px) {
    .label {
      display: none;
    }
    .actions .primary {
      padding: 6px 12px;
    }
  }

  /* Narrow phones (mini, SE, Display Zoom): tighter, still tappable. */
  @media (max-width: 389px) {
    header {
      gap: 8px;
      padding-left: 12px;
      padding-right: 6px;
    }
    .actions .icon,
    .actions .plain {
      min-width: 38px;
      padding-left: 4px;
      padding-right: 4px;
    }
    .actions .primary {
      margin: 0 3px;
      padding: 6px 10px;
    }
  }

  @media (max-width: 340px) {
    h1 {
      font-size: 1.12rem;
    }
    .actions .icon,
    .actions .plain {
      min-width: 34px;
    }
  }
</style>
