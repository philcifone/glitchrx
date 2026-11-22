<script lang="ts">
  // Bottom line: photo facts, render time, messages.
  import { app } from '../lib/app.svelte';

  const mb = (n: number) => (n >= 1 << 20 ? `${(n / (1 << 20)).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`);
</script>

<footer>
  {#if app.error}
    <span class="err" role="alert">
      {app.error}
      <button class="dismiss" onclick={() => app.dismissError()} aria-label="Dismiss">ok</button>
    </span>
  {:else if app.ready}
    <span class="ready" role="status">
      <span class="file">{app.ready.name} is ready</span>
      <button class="primary save" onclick={() => app.share()}>Save</button>
      <button class="plain dismiss-ready" onclick={() => (app.ready = null)} aria-label="Discard">Discard</button>
    </span>
  {:else if app.notice}
    <span class="ok" role="status">{app.notice}</span>
  {:else if app.doc}
    <span class="name" title={app.doc.path ?? app.doc.name}>{app.doc.name}</span>
    <span>{app.doc.width}x{app.doc.height}</span>
    <span class="wide">
      {app.doc.format}{app.doc.orientation !== 1 ? ', turned upright' : ''}{app.doc.has_icc ? ', ICC' : ''},
      {mb(app.doc.raw_len)} raw
    </span>
    {#if app.heavy}
      <span class="warn" title={app.heavy}>too heavy live</span>
    {:else if app.renderMs !== null}
      <span class="dim">{app.renderMs} ms{app.exact ? ', exact' : ''}</span>
    {/if}
  {:else}
    <span class="dim">no photo open{app.config ? ` | glitchrx ${app.config.version}` : ''}</span>
  {/if}
</footer>

<style>
  footer {
    grid-area: status;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 6px var(--pad);
    padding-bottom: max(6px, env(safe-area-inset-bottom));
    border-top: var(--hairline);
    background: var(--bar);
    color: var(--fg-dim);
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
    min-height: 30px;
    position: relative;
    z-index: 2;
    white-space: nowrap;
    overflow: hidden;
  }

  .name {
    font-weight: 600;
    color: var(--fg);
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }

  .ready {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    flex: 1;
    color: var(--fg);
  }

  .file {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .save {
    min-height: 30px;
    padding: 0 16px;
  }

  .dismiss-ready {
    min-height: 30px;
    color: var(--fg-dim);
  }

  .warn {
    color: var(--warn);
  }

  .err {
    color: var(--danger);
    white-space: normal;
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .ok {
    color: var(--accent);
    white-space: normal;
  }

  .dismiss {
    min-height: 24px;
    padding: 0 10px;
    border-radius: 999px;
  }

  @media (max-width: 760px) {
    .wide {
      display: none;
    }
  }
</style>
