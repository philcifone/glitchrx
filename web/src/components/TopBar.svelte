<script lang="ts">
  // Title, open, presets, undo/redo, export, settings. While a mask is
  // being painted it becomes an edit bar, as in iOS: what is being masked,
  // undo/redo (used constantly while painting) and Done. Nothing floats
  // over the photo, and Open and Export wait until painting is done.
  import FolderOpen from '@lucide/svelte/icons/folder-open';
  import Bookmark from '@lucide/svelte/icons/bookmark';
  import Download from '@lucide/svelte/icons/download';
  import Undo2 from '@lucide/svelte/icons/undo-2';
  import Redo2 from '@lucide/svelte/icons/redo-2';
  import SquareSplitHorizontal from '@lucide/svelte/icons/square-split-horizontal';
  import Settings from '@lucide/svelte/icons/settings';
  import Eye from '@lucide/svelte/icons/eye';
  import EyeOff from '@lucide/svelte/icons/eye-off';
  import { app } from '../lib/app.svelte';

  let {
    onopen,
    onexport,
    onpresets,
    onsettings,
  }: { onopen: () => void; onexport: () => void; onpresets: () => void; onsettings: () => void } = $props();
</script>

<header>
  {#if app.maskTarget || app.pullTarget}
    {@const t = (app.maskTarget ?? app.pullTarget)!}
    {@const pull = !app.maskTarget}
    <div class="mode" role="status">
      <h1>{pull ? 'Pull' : 'Mask'}</h1>
      <span class="what">Step {t.n}: {t.step.effect}{t.step.enabled ? '' : ' (off)'}</span>
    </div>
    <div class="actions">
      <!-- The red overlay (rubylith) on and off, to see the picture while
           masking; the same setting as Show mask in the Blend section
           (Phil, 2026-11-25). -->
      {#if !pull}
        <button
          class="icon"
          onclick={() => app.setTool({ show: !app.tool.show })}
          aria-pressed={!app.tool.show}
          title={app.tool.show ? 'Hide the red overlay' : 'Show the red overlay'}
          aria-label={app.tool.show ? 'Hide the mask overlay' : 'Show the mask overlay'}
        >
          {#if app.tool.show}<Eye size={18} strokeWidth={1.75} />{:else}<EyeOff size={18} strokeWidth={1.75} />{/if}
        </button>
      {/if}
      <!-- Holding the photo paints here, so comparing has its own button. -->
      <button
        class="icon compare"
        class:on={app.comparing}
        onpointerdown={(e) => {
          (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
          app.compare(true);
        }}
        onpointerup={() => app.compare(false)}
        onpointercancel={() => app.compare(false)}
        onlostpointercapture={() => app.compare(false)}
        oncontextmenu={(e) => e.preventDefault()}
        title="Hold to see the original (or hold the \ key)"
        aria-label="Hold to see the original"
      >
        <!-- The disc that fills while held: a circle centred on the icon,
             whatever shape the button's touch area is. -->
        <span class="disc"><SquareSplitHorizontal size={18} strokeWidth={1.75} /></span>
      </button>
      <button class="icon" onclick={() => app.undo()} disabled={!app.canUndo} title="Undo the last stroke (Ctrl+Z)" aria-label="Undo">
        <Undo2 size={18} strokeWidth={1.75} />
      </button>
      <button class="icon" onclick={() => app.redo()} disabled={!app.canRedo} title="Redo (Ctrl+Shift+Z)" aria-label="Redo">
        <Redo2 size={18} strokeWidth={1.75} />
      </button>
      <button
        class="primary"
        onclick={() => (pull ? app.stopPulling() : app.stopMasking())}
        title={pull ? 'Finish pulling (Enter)' : 'Finish painting (Enter)'}>Done</button
      >
    </div>
  {:else}
    <h1>glitchRx</h1>
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
      <button class="icon" onclick={onsettings} title="Settings: appearance, folders" aria-label="Settings">
        <Settings size={18} strokeWidth={1.75} />
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

  /* The compare button. Held, its disc fills at once: one change, no
     fades. A dim on press, a fade of the fill and an instant change of the
     icon's color all overlapped on iOS, and the icon showed twice, offset,
     halfway through (Phil, 2026-11-24). */
  .actions .compare,
  .actions .compare:active:not(:disabled) {
    opacity: 1;
    transition: none;
    background: none;
  }

  .disc {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
  }

  .compare.on .disc {
    background: var(--accent);
    color: var(--accent-fg);
  }

  .actions .icon {
    touch-action: none;
    -webkit-touch-callout: none;
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

  /* Phones up to the Pro Max (430 px): tighter, still tappable. */
  @media (max-width: 429px) {
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
