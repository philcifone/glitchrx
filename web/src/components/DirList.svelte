<script lang="ts">
  // The folders inside one folder, as a plain list like a file manager's:
  // `..` first (the folder above, when there is one), then each folder.
  // Used by the Open dialog and the folder picker.
  import Folder from '@lucide/svelte/icons/folder';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';

  let {
    dirs,
    parent,
    ongo,
  }: {
    dirs: { name: string; path: string }[];
    /** The folder above, or null at the top. */
    parent: string | null;
    ongo: (path: string) => void;
  } = $props();
</script>

{#if parent !== null || dirs.length}
  <ul class="dirs">
    {#if parent !== null}
      <li>
        <button onclick={() => ongo(parent ?? '')} aria-label="The folder above" title="The folder above">
          <Folder size={16} strokeWidth={1.75} />
          <span class="name">..</span>
        </button>
      </li>
    {/if}
    {#each dirs as d (d.path)}
      <li>
        <button onclick={() => ongo(d.path)}>
          <Folder size={16} strokeWidth={1.75} />
          <span class="name">{d.name}</span>
          <ChevronRight class="go" size={16} strokeWidth={1.75} />
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .dirs {
    list-style: none;
    margin: 0;
    padding: 0;
    background: var(--card);
    border-radius: 12px;
    overflow: hidden;
  }

  li + li {
    border-top: var(--hairline);
  }

  button {
    width: 100%;
    justify-content: flex-start;
    gap: 10px;
    border-radius: 0;
    background: none;
    font-weight: 400;
    min-height: 44px;
  }

  .name {
    flex: 1;
    min-width: 0;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  button :global(svg) {
    color: var(--accent);
    flex: none;
  }

  button :global(svg.go) {
    color: var(--fg-dim);
  }
</style>
