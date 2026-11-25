<script lang="ts">
  // A `set` parameter (D-57): any of a few options, as a row of toggles.
  // Color names (hsl's `range`) show as color dots. Tap one to turn it
  // on, again to turn it off; several on work together. None on is `all`.
  import Check from '@lucide/svelte/icons/check';

  let {
    id,
    options,
    text,
    onchange,
  }: {
    id: string;
    options: string[];
    /** `all`, or options joined by `+`. */
    text: string;
    /** The new value, in list order (`all` when none is on). */
    onchange: (value: string) => void;
  } = $props();

  /** Muted Gruvbox, one per color range. */
  const SWATCH: Record<string, string> = {
    reds: '#cc241d',
    yellows: '#d79921',
    greens: '#98971a',
    cyans: '#689d6a',
    blues: '#458588',
    magentas: '#b16286',
  };

  const on = $derived(new Set(text === 'all' ? [] : text.split('+').map((s) => s.trim())));

  function toggle(o: string) {
    const next = new Set(on);
    if (next.has(o)) next.delete(o);
    else next.add(o);
    const picked = options.filter((x) => next.has(x));
    onchange(picked.length ? picked.join('+') : 'all');
  }

  const label = (o: string) => o.charAt(0).toUpperCase() + o.slice(1);
</script>

<div {id} class="set" role="group" aria-label="Colors">
  {#each options as o (o)}
    {@const color = SWATCH[o]}
    <button
      class="opt"
      class:dot={!!color}
      aria-pressed={on.has(o)}
      onclick={() => toggle(o)}
      title={label(o)}
      aria-label={label(o)}
      style:--swatch={color}
    >
      {#if color}
        {#if on.has(o)}<Check size={14} strokeWidth={2.5} />{/if}
      {:else}
        {o}
      {/if}
    </button>
  {/each}
  <span class="what dim">{on.size ? options.filter((o) => on.has(o)).map(label).join(', ') : 'All colors'}</span>
</div>

<style>
  .set {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .opt.dot {
    width: 30px;
    height: 30px;
    min-height: 30px;
    padding: 0;
    border-radius: 50%;
    border: none;
    background: var(--swatch);
    color: #fbf1c7;
    display: grid;
    place-items: center;
    /* Off: a quieter dot. On: full color with a ring. */
    opacity: 0.45;
    transition:
      opacity 0.12s,
      box-shadow 0.12s;
  }

  .opt.dot[aria-pressed='true'] {
    opacity: 1;
    box-shadow:
      0 0 0 2px var(--card),
      0 0 0 4px var(--swatch);
  }

  .what {
    flex: 1;
    min-width: 8ch;
    font-size: 0.8rem;
  }

  @media (pointer: coarse) {
    .opt.dot {
      width: 34px;
      height: 34px;
    }
  }
</style>
