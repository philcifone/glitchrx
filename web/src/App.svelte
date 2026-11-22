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

  let openDialog: OpenDialog;
  let exportDialog: ExportDialog;
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
    }
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
  <TopBar onopen={() => openDialog.show()} onexport={() => exportDialog.show()} />
  <main class="stage">
    <Preview onopen={() => openDialog.show()} />
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
    outline: 4px dashed var(--accent);
    outline-offset: -4px;
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
    border-left: var(--border);
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

    .sheet {
      border-left: none;
      border-top: var(--border);
    }

    .handle {
      display: flex;
      min-height: 22px;
      padding: 0;
      border: none;
      background: var(--bg);
      width: 100%;
      flex: none;
    }

    .handle span {
      width: 48px;
      height: 4px;
      background: var(--fg-dim);
    }
  }
</style>
