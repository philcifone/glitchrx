<script lang="ts">
  // Layout shell: top bar, preview stage, step stack, status bar.
  // Desktop: stage centre, stack on the right. Phone: stage on top, the
  // stack as a bottom sheet that can be expanded.
  import { onMount } from 'svelte';
  import { app } from './lib/app.svelte';
  import { applySavedTheme } from './lib/theme';
  import TopBar from './components/TopBar.svelte';
  import Preview from './components/Preview.svelte';
  import StepStack from './components/StepStack.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import OpenDialog from './components/OpenDialog.svelte';
  import ExportDialog from './components/ExportDialog.svelte';
  import PresetsDialog from './components/PresetsDialog.svelte';
  import NewPhotoDialog from './components/NewPhotoDialog.svelte';
  import SettingsDialog from './components/SettingsDialog.svelte';

  let openDialog: OpenDialog;

  /** The system's Open dialog in the desktop app, ours in a browser. */
  function open() {
    if (app.desktop) void app.openNative();
    else openDialog.show();
  }
  let exportDialog: ExportDialog;
  let presetsDialog: PresetsDialog;
  let settingsDialog: SettingsDialog;
  let sheetExpanded = $state(false);

  onMount(() => {
    applySavedTheme();
    void app.init();
  });

  /** Undo/redo shortcuts, except while typing in a field. */
  function onKey(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (el.closest('input[type="text"], input[type="number"], textarea, select')) return;
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey) app.redo();
      else app.undo();
    } else if (mod && e.key.toLowerCase() === 'y') {
      e.preventDefault();
      app.redo();
    } else if (e.key === '\\' && !e.repeat) {
      app.compare(true);
    } else if (!mod && !e.altKey && e.key.toLowerCase() === 'z' && app.doc) {
      e.preventDefault();
      app.toggleZoom(); // fit to 100% and back, as in Lightroom
    } else if (app.masking && !mod && !e.altKey) {
      maskKey(e);
    }
  }

  /**
   * Mask editing shortcuts, Photoshop style: B and E paint and erase with
   * a brush (the active one, else the last, else a new one), [ and ] size
   * it, Enter or Esc finish (a pen path being drawn first, then editing),
   * Delete removes the picked pen point.
   */
  function maskKey(e: KeyboardEvent) {
    const uid = app.masking!;
    const k = e.key.toLowerCase();
    if (k === 'b' || k === 'e') {
      const ops = app.maskTarget?.step.mask?.op ?? [];
      if (app.activeOp === null || ops[app.activeOp]?.shape !== 'brush') {
        let last = ops.length - 1;
        while (last >= 0 && ops[last].shape !== 'brush') last--;
        if (last >= 0) app.selectComponent(uid, last);
        else app.addComponent(uid, 'brush');
      }
      app.setTool({ tool: k === 'e' ? 'erase' : 'paint' });
    } else if (e.key === '[') app.setTool({ size: Math.max(0.005, Math.round(app.tool.size * 0.85 * 10000) / 10000) });
    else if (e.key === ']') app.setTool({ size: Math.min(0.3, Math.round(app.tool.size * 1.18 * 10000) / 10000) });
    else if ((e.key === 'Escape' || e.key === 'Enter') && app.penPlacing) app.finishPen();
    else if (e.key === 'Escape' || e.key === 'Enter') app.stopMasking();
    else if ((e.key === 'Delete' || e.key === 'Backspace') && app.penPoint !== null) app.removePenPoint();
    else return;
    e.preventDefault();
  }

  function onKeyUp(e: KeyboardEvent) {
    if (e.key === '\\') app.compare(false);
  }

  // Drop a photo anywhere to open it (desktop).
  let dragging = $state(false);
  function onDrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    const file = e.dataTransfer?.files[0];
    if (file) void app.openFile(file);
  }
</script>

<svelte:window onkeydown={onKey} onkeyup={onKeyUp} />

<div
  class="app"
  class:expanded={sheetExpanded}
  class:dragging
  role="application"
  ondragover={(e) => {
    e.preventDefault();
    dragging = true;
  }}
  ondragleave={(e) => {
    if (e.target === e.currentTarget) dragging = false;
  }}
  ondrop={onDrop}
>
  <TopBar onopen={open} onexport={() => exportDialog.show()} onpresets={() => presetsDialog.show()} onsettings={() => settingsDialog.show()} />
  <main class="stage">
    <Preview onopen={open} />
  </main>
  <aside class="sheet" aria-label="Steps">
    <button
      class="handle"
      aria-label={sheetExpanded ? 'Shrink the step panel' : 'Expand the step panel'}
      aria-expanded={sheetExpanded}
      onclick={() => (sheetExpanded = !sheetExpanded)}
    >
      <span></span>
    </button>
    <StepStack />
  </aside>
  <StatusBar />
</div>

<OpenDialog bind:this={openDialog} />
<ExportDialog bind:this={exportDialog} />
<PresetsDialog bind:this={presetsDialog} />
<NewPhotoDialog />
<SettingsDialog bind:this={settingsDialog} />

<style>
  .app {
    display: grid;
    height: 100dvh;
    grid-template-columns: minmax(0, 1fr) 380px;
    grid-template-rows: auto minmax(0, 1fr) auto;
    grid-template-areas:
      'top top'
      'stage sheet'
      'status status';
  }

  .app.dragging {
    outline: 3px dashed var(--accent);
    outline-offset: -6px;
  }

  .stage {
    grid-area: stage;
    min-height: 0;
    background: var(--stage);
    position: relative;
  }

  .sheet {
    grid-area: sheet;
    min-height: 0;
    border-left: var(--hairline);
    background: var(--bg);
    display: flex;
    flex-direction: column;
  }

  .handle {
    display: none;
  }

  /* Phone: the stack becomes a bottom sheet under the stage. */
  @media (max-width: 760px) {
    .app {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 52fr) minmax(0, 48fr) auto;
      grid-template-areas: 'top' 'stage' 'sheet' 'status';
    }

    .app.expanded {
      grid-template-rows: auto minmax(0, 22fr) minmax(0, 78fr) auto;
    }

    /* A bottom sheet: rounded top corners over the stage, with a grabber. */
    .sheet {
      border-left: none;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      margin-top: calc(-1 * var(--radius-lg));
      box-shadow: 0 -4px 20px rgb(0 0 0 / 0.12);
      position: relative;
      z-index: 1;
    }

    .handle {
      display: flex;
      min-height: 22px;
      padding: 0;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      background: none;
      width: 100%;
      flex: none;
    }

    .handle:hover:not(:disabled) {
      background: none;
    }

    .handle span {
      width: 36px;
      height: 5px;
      border-radius: 3px;
      background: var(--fill-strong);
    }
  }
</style>
