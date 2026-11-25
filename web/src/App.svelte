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
  import { stepSize } from './lib/pull';

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

  /** The photo fills the screen (not while masking or cropping). */
  const full = $derived(app.fullView && !app.drawing && !app.cropping);

  // The bars' own heights, so the layout can animate them away in full
  // view: grid rows animate between lengths, not from `auto`.
  let topH = $state(0);
  let statusH = $state(0);

  // The grabber (phone). The panel follows the finger, and the photo grows
  // or shrinks with it; on release it eases to the nearest of three places:
  // expanded, normal, or down to just its grabber (full view), as an iOS
  // sheet does. A tap expands or shrinks it, or, in full view, brings it
  // back.
  //
  // Places are the stage's share of the stage and panel together, in fr
  // out of 100, which is also how the rows are written below.
  type Place = 'expanded' | 'normal' | 'full';
  const SHARE: Record<Place, number> = { expanded: 22, normal: 52, full: 100 };
  const ORDER: Place[] = ['expanded', 'normal', 'full'];
  const SWIPE = 36;
  const TAP = 6;
  let grab: { y: number; from: Place; share: number; span: number } | null = null;
  /** The stage's share while a finger drags the panel. */
  let dragShare = $state<number | null>(null);

  const place = $derived<Place>(full ? 'full' : sheetExpanded ? 'expanded' : 'normal');

  function grabDown(e: PointerEvent) {
    const handle = e.currentTarget as HTMLElement;
    const stage = document.querySelector('.stage')?.getBoundingClientRect();
    const sheet = handle.closest('.sheet')?.getBoundingClientRect();
    if (!stage || !sheet) return;
    const span = stage.height + sheet.height;
    grab = { y: e.clientY, from: place, share: (stage.height / span) * 100, span };
    dragFrom = place;
    handle.setPointerCapture(e.pointerId);
  }

  function grabMove(e: PointerEvent) {
    if (!grab) return;
    const dy = e.clientY - grab.y;
    if (dragShare === null && Math.abs(dy) < TAP) return;
    dragShare = Math.min(100, Math.max(12, grab.share + (dy / grab.span) * 100));
  }

  function grabUp(e: PointerEvent) {
    if (!grab) return;
    const dy = e.clientY - grab.y;
    const from = grab.from;
    const at = dragShare;
    grab = null;
    dragShare = null;
    if (at === null) {
      tapHandle(from);
      return;
    }
    // The nearest place; a real swipe that would land back where it
    // started goes one place on, in its direction.
    let to = ORDER.reduce((a, b) => (Math.abs(SHARE[b] - at) < Math.abs(SHARE[a] - at) ? b : a));
    if (to === from && Math.abs(dy) > SWIPE) {
      const i = ORDER.indexOf(from) + Math.sign(dy);
      to = ORDER[Math.min(ORDER.length - 1, Math.max(0, i))];
    }
    goTo(to);
  }

  /** A tap on the grabber: back from full view, else expand or shrink. */
  function tapHandle(from: Place) {
    if (from === 'full') goTo('normal');
    else sheetExpanded = !sheetExpanded;
  }

  function goTo(p: Place) {
    if (p === 'full') {
      app.fullView = true;
      return;
    }
    app.fullView = false;
    sheetExpanded = p === 'expanded';
  }

  /** Where a drag started; the bars stay away while one leaves full view. */
  let dragFrom = $state<Place>('normal');

  /** Rows while dragging; otherwise the classes below decide. */
  const dragRows = $derived.by(() => {
    if (dragShare === null) return undefined;
    const [top, status] = dragFrom === 'full' ? ['0px', '0px'] : ['var(--top-h, auto)', 'var(--status-h, auto)'];
    return `${top} minmax(0, ${dragShare}fr) minmax(var(--peek), ${100 - dragShare}fr) ${status}`;
  });

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
    } else if (app.pullTarget && !mod && !e.altKey) {
      pullKey(e);
    } else if (app.masking && !mod && !e.altKey) {
      maskKey(e);
    } else if (e.key === 'Escape' && app.fullView) {
      app.fullView = false;
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

  /**
   * Pull mode: [ and ] size the selected brush or pin (or the next one),
   * Delete removes it, Enter or Esc finish.
   */
  function pullKey(e: KeyboardEvent) {
    const now = app.selectedBrush?.first.size ?? app.selectedPin?.pin.size ?? app.pullTool.size;
    if (e.key === '[' || e.key === ']') app.adjustPull({ size: stepSize(now, e.key === '[' ? -1 : 1) }, true);
    else if ((e.key === 'Delete' || e.key === 'Backspace') && (app.selectedBrush || app.selectedPin)) app.deleteSelected();
    else if (e.key === 'Escape' || e.key === 'Enter') app.stopPulling();
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
  class:full
  class:grabbing={dragShare !== null}
  style:--top-h={topH ? `${topH}px` : undefined}
  style:--status-h={statusH ? `${statusH}px` : undefined}
  style:grid-template-rows={dragRows}
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
  <!-- The bars sit in clipping wrappers, so their rows can shrink to
       nothing and slide them off the screen. -->
  <div class="bar top">
    <div class="bar-in" bind:offsetHeight={topH}>
      <TopBar onopen={open} onexport={() => exportDialog.show()} onpresets={() => presetsDialog.show()} onsettings={() => settingsDialog.show()} />
    </div>
  </div>
  <main class="stage">
    <Preview onopen={open} />
  </main>
  <aside class="sheet" aria-label="Steps">
    <button
      class="handle"
      aria-label={full ? 'Show the step panel' : sheetExpanded ? 'Shrink the step panel' : 'Expand the step panel'}
      aria-expanded={full ? false : sheetExpanded}
      onpointerdown={grabDown}
      onpointermove={grabMove}
      onpointerup={grabUp}
      onpointercancel={() => {
        grab = null;
        dragShare = null;
      }}
      onclick={(e) => {
        // Pointer taps are handled on pointerup; this is the keyboard.
        if (e.detail === 0) tapHandle(place);
      }}
    >
      <span></span>
    </button>
    <!-- Fades while the panel is down to its grabber. -->
    <div class="sheet-body">
      <StepStack />
    </div>
  </aside>
  <div class="bar status">
    <div class="bar-in" bind:offsetHeight={statusH}>
      <StatusBar />
    </div>
  </div>
</div>

<OpenDialog bind:this={openDialog} />
<ExportDialog bind:this={exportDialog} />
<PresetsDialog bind:this={presetsDialog} />
<NewPhotoDialog />
<SettingsDialog bind:this={settingsDialog} />

<style>
  /* The layout is one grid whose rows (and, on a desktop, the panel's
     column) animate between places: the bars' rows between their height
     and 0, the stage and panel between shares of what is left, in fr out
     of 100. Rows interpolate only when every track has the same kind of
     size on both sides, which is why the bars are measured rather than
     left at auto. */
  .app {
    --ease: cubic-bezier(0.32, 0.72, 0, 1); /* an iOS sheet's settle */
    display: grid;
    height: 100dvh;
    grid-template-columns: minmax(0, 1fr) 380px;
    grid-template-rows: var(--top-h, auto) minmax(0, 1fr) var(--status-h, auto);
    grid-template-areas:
      'top top'
      'stage sheet'
      'status status';
    transition:
      grid-template-rows 0.38s var(--ease),
      grid-template-columns 0.38s var(--ease);
  }

  /* The finger leads while dragging. */
  .app.grabbing {
    transition: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .app {
      transition: none;
    }
  }

  .app.dragging {
    outline: 3px dashed var(--accent);
    outline-offset: -6px;
  }

  .bar {
    overflow: hidden;
    min-height: 0;
    position: relative;
    z-index: 2;
    display: flex;
    flex-direction: column;
  }

  .bar.top {
    grid-area: top;
    /* Shrinking slides it up, off the top. */
    justify-content: flex-end;
  }

  .bar.status {
    grid-area: status;
  }

  .bar-in {
    flex: none;
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
    min-width: 0;
    overflow: hidden;
    border-left: var(--hairline);
    background: var(--bg);
    display: flex;
    flex-direction: column;
  }

  .handle {
    display: none;
  }

  .sheet-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    transition: opacity 0.25s var(--ease);
  }

  .app.full .sheet-body {
    opacity: 0;
    pointer-events: none;
  }

  /* Full view: the bars and the panel shrink away; only the stage stays. */
  .app.full {
    grid-template-columns: minmax(0, 1fr) 0px;
    grid-template-rows: 0px minmax(0, 1fr) 0px;
  }

  .app.full .sheet {
    border-left: none;
  }

  /* Phone: the stack becomes a bottom sheet under the stage. Its row
     never goes below the peek, the grabber's height: in full view the
     panel stays as just its grabber, to pull it back up by. */
  @media (max-width: 760px) {
    .app {
      --peek: calc(22px + env(safe-area-inset-bottom));
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: var(--top-h, auto) minmax(0, 52fr) minmax(var(--peek), 48fr) var(--status-h, auto);
      grid-template-areas: 'top' 'stage' 'sheet' 'status';
    }

    .app.expanded {
      grid-template-rows: var(--top-h, auto) minmax(0, 22fr) minmax(var(--peek), 78fr) var(--status-h, auto);
    }

    .app.full {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: 0px minmax(0, 100fr) minmax(var(--peek), 0fr) 0px;
    }

    /* A bottom sheet: rounded top corners over the stage, with a grabber. */
    .sheet {
      border-left: none;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      margin-top: calc(-1 * var(--radius-lg));
      box-shadow: 0 -4px 20px rgb(0 0 0 / 0.12);
      position: relative;
      z-index: 1;
      transition:
        margin-top 0.38s var(--ease),
        box-shadow 0.38s var(--ease);
    }

    .handle {
      display: flex;
      min-height: 22px;
      padding: 0;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      background: none;
      width: 100%;
      flex: none;
      /* The drag is ours, not a scroll. */
      touch-action: none;
      cursor: grab;
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
