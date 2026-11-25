<script lang="ts">
  // The photo's toolbox, at the top of the step panel: crop, the zoom, and
  // the work size. They used to float over the photo, where they got in
  // the way of looking at it (Phil, 2026-11-24); here they sit with the
  // other recipe-wide settings, in a card beside the encoding's: crop,
  // zoom and an info button on top, the work size below. The info button
  // shows the work size's note under the cards (`info`), inline like the
  // encoding's, rather than in a popup the app has nowhere else.
  import CropIcon from '@lucide/svelte/icons/crop';
  import ZoomIn from '@lucide/svelte/icons/zoom-in';
  import ZoomOut from '@lucide/svelte/icons/zoom-out';
  import Info from '@lucide/svelte/icons/info';
  import { tip } from '../lib/hold';
  import { app } from '../lib/app.svelte';
  import { WORK_SIZES } from '../lib/canvas';
  import { percent } from '../lib/view';

  let { info = $bindable(false) }: { info?: boolean } = $props();

  const g = $derived(app.geometry);
  /** Crop has the stage while it is open; the rest waits for Done. */
  const busy = $derived(!!app.cropping);
</script>

<section class="card tools" aria-label="Photo">
  <div class="line">
  <button
    class="tool"
    class:on={!!app.recipe.crop}
    onclick={() => app.startCrop()}
    disabled={busy}
    title="Crop: keep part of the photo, or of the result"
    aria-label="Crop"
  >
    <CropIcon size={18} strokeWidth={1.75} />
  </button>
  <button
    class="tool zoom"
    class:on={app.zoomed}
    onclick={() => app.toggleZoom()}
    disabled={busy || !g}
    title={app.zoomed
      ? 'Back to fit (Z, or double tap)'
      : 'Zoom to 100% (Z, or double tap; pinch to zoom). Press and hold the photo for the original.'}
    aria-label={app.zoomed ? 'Zoom to fit' : 'Zoom to 100%'}
  >
    {#if app.zoomed && g}
      <ZoomOut size={18} strokeWidth={1.75} />
      <span>{percent(app.view, g)}%</span>
    {:else}
      <ZoomIn size={18} strokeWidth={1.75} />
    {/if}
  </button>
  <button
    class="tool info"
    class:on={info}
    {@attach tip}
    onclick={() => (info = !info)}
    aria-pressed={info}
    title={info ? 'Hide the note on sizes' : 'What the sizes do'}
    aria-label={info ? 'Hide the note on sizes' : 'What the sizes do'}
  >
    <Info size={18} strokeWidth={1.75} />
  </button>
  </div>
  <!-- Work size (5b): the size the steps run on. Smaller is faster and
       exact at once, with coarser glitch texture. -->
  <div
    class="segmented work"
    role="radiogroup"
    aria-label="Work size"
    title="The size the steps run on: smaller is faster, with coarser texture"
  >
    {#each WORK_SIZES as ws (ws.label)}
      <button
        role="radio"
        aria-checked={(app.recipe.work_size ?? null) === ws.size}
        class:on={(app.recipe.work_size ?? null) === ws.size}
        onclick={() => app.setWorkSize(ws.size)}
        disabled={busy || (ws.size !== null && !!app.croppedSize && Math.max(app.croppedSize.w, app.croppedSize.h) <= ws.size)}
      >
        {ws.label}
      </button>
    {/each}
  </div>
</section>

<style>
  .card {
    background: var(--card);
    border-radius: var(--radius-lg);
    padding: 10px 12px;
  }

  /* Crop, zoom and info on top; the work size below, the card's width. */
  .tools {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 8px;
    padding: 10px;
  }

  .line {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .info {
    margin-left: auto;
  }

  .tool {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 34px;
    min-width: 40px;
    justify-content: center;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 0.9rem;
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }

  .tool :global(svg) {
    color: var(--accent);
    flex: none;
  }

  /* On: the photo is cropped, or zoomed in. */
  .tool.on {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .work {
    display: flex;
  }

  .work button {
    flex: 1;
    min-width: 0;
    padding-left: 2px;
    padding-right: 2px;
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
  }
</style>
