<script lang="ts">
  // The preview stage: shows the latest render (or the original while
  // comparing), reports its size to the app, draws the region band while
  // one is dragged, and hosts the compare and exact buttons.
  import { onMount } from 'svelte';
  import { app } from '../lib/app.svelte';

  let { onopen }: { onopen: () => void } = $props();
  let box: HTMLDivElement;
  let img = $state<HTMLImageElement>();
  // The image's box inside the stage, for placing the region band. Kept up
  // to date by the observer below, since the image resizes with the stage.
  let rect = $state({ x: 0, y: 0, w: 0, h: 0 });
  function measure() {
    if (img) rect = { x: img.offsetLeft, y: img.offsetTop, w: img.offsetWidth, h: img.offsetHeight };
  }

  onMount(() => {
    // Tell the app how big the preview area is, so renders fit it.
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      app.setViewport(Math.round(width), Math.round(height), window.devicePixelRatio || 1);
      measure();
    });
    ro.observe(box);
    return () => ro.disconnect();
  });

  const shown = $derived(app.comparing && app.originalUrl ? app.originalUrl : app.imageUrl);

  // Press and hold to see the original. Pointer events cover mouse, touch
  // and pen; capture keeps the release even if the finger slides off.
  function hold(e: PointerEvent) {
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    app.compare(true);
  }
  const release = () => app.compare(false);
</script>

<div class="box" bind:this={box}>
  {#if shown}
    <img
      bind:this={img}
      src={shown}
      alt={app.comparing ? 'Original photo' : 'Glitched preview'}
      draggable="false"
      onload={measure}
    />
    {#if app.regionHint}
      <!-- The region is a share of the raster's bytes, and rows follow each
           other, so it is the same share of the height: a horizontal band. -->
      {@const r = app.regionHint}
      <div
        class="band"
        aria-hidden="true"
        style:left="{rect.x}px"
        style:top="{rect.y}px"
        style:width="{rect.w}px"
        style:height="{rect.h}px"
      >
        <div class="shade" style:top="0" style:height="{r.s}%"></div>
        <div class="edge" style:top="{r.s}%" style:height="{r.e - r.s}%"></div>
        <div class="shade" style:top="{r.e}%" style:bottom="0"></div>
      </div>
    {/if}
  {:else if !app.doc}
    <div class="empty">
      <p>Open a photo to start: JPEG, PNG or TIFF.</p>
      <button class="primary" onclick={onopen}>Open a photo</button>
      <p class="dim hint">Or drop one here. Photos go to this glitchrx server only.</p>
    </div>
  {/if}

  {#if app.busy || app.working}
    <div class="progress" aria-hidden="true"></div>
  {/if}
  {#if app.working}
    <div class="working" role="status">{app.working}</div>
  {:else if app.heavy}
    <div class="working heavy" role="status">
      {app.heavy}.
      <button onclick={() => app.render('exact')}>Try exact</button>
    </div>
  {/if}

  {#if app.doc && app.imageUrl}
    <button
      class="exact"
      class:on={app.exact}
      onclick={() => app.render('exact')}
      disabled={app.exact || !!app.working}
      title="Render at full resolution, exactly like the export. Slower; long reverb tails differ most in the live preview."
    >
      {app.exact ? 'Exact' : 'Make exact'}
    </button>
    <button
      class="compare"
      class:on={app.comparing}
      onpointerdown={hold}
      onpointerup={release}
      onpointercancel={release}
      onlostpointercapture={release}
      oncontextmenu={(e) => e.preventDefault()}
      title="Hold to see the original (or hold the \ key)"
    >
      {app.comparing ? 'Original' : 'Hold: original'}
    </button>
  {/if}
</div>

<style>
  .box {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
    display: block;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }

  .empty {
    text-align: center;
    padding: var(--pad);
    color: var(--fg);
  }

  .hint {
    font-size: 0.9em;
    max-width: 32ch;
    margin-inline: auto;
  }

  /* An indeterminate bar along the top edge while rendering. */
  .progress {
    position: absolute;
    top: 0;
    left: 0;
    height: 3px;
    width: 30%;
    background: var(--accent);
    animation: slide 0.9s linear infinite;
  }

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .progress {
      animation: none;
      width: 100%;
      opacity: 0.6;
    }
  }

  .working {
    position: absolute;
    top: 12px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--bg);
    border: var(--border);
    padding: 4px 10px;
  }

  /* Region band: shade outside it, bold edges along it. */
  .band {
    position: absolute;
    pointer-events: none;
  }

  .shade {
    position: absolute;
    left: 0;
    right: 0;
    background: rgb(0 0 0 / 0.55);
  }

  .edge {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 3px solid var(--accent);
    border-bottom: 3px solid var(--accent);
  }

  .heavy {
    border-color: var(--warn);
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: calc(100% - 24px);
  }

  .exact {
    position: absolute;
    left: 12px;
    bottom: 12px;
    background: var(--bg);
  }

  .exact.on {
    opacity: 1;
    color: var(--accent);
    border-color: var(--accent);
  }

  .compare {
    position: absolute;
    right: 12px;
    bottom: 12px;
    background: var(--bg);
    touch-action: none;
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
  }

  .compare.on {
    background: var(--accent);
    color: var(--accent-fg);
    border-color: var(--accent);
  }
</style>
