<script lang="ts">
  // Opening another photo while the stack has steps: keep them, or start
  // fresh? Presets are the way to reuse a look on purpose; this only asks
  // about the steps left over. Starting fresh can be undone, and the
  // choice can be remembered (it can be changed in Presets).
  import { app } from '../lib/app.svelte';

  let dialog: HTMLDialogElement;
  let remember = $state(false);

  $effect(() => {
    if (app.askNewPhoto > 0 && !dialog.open) {
      remember = false;
      dialog.showModal();
    }
  });

  function choose(fresh: boolean) {
    if (remember) app.setNewPhotoChoice(fresh ? 'fresh' : 'keep');
    if (fresh) app.startFresh();
    else app.keepSteps();
    dialog.close();
  }

  const n = $derived(app.askNewPhoto);
</script>

<!-- Escape (cancel) keeps the steps: nothing changes. -->
<dialog bind:this={dialog} aria-labelledby="new-photo-title" onclose={() => app.keepSteps()}>
  <div class="body">
    <h2 id="new-photo-title">New photo</h2>
    <p>
      Keep the {n === 1 ? 'step' : `${n} steps`} from the last photo, or start fresh? Starting fresh can be undone.
    </p>
    <label class="row">
      <span>Don't ask again</span>
      <input class="switch" type="checkbox" bind:checked={remember} />
    </label>
    {#if remember}
      <p class="dim small">You can change this later in Presets.</p>
    {/if}
    <div class="buttons">
      <button onclick={() => choose(false)}>Keep {n === 1 ? 'step' : 'steps'}</button>
      <button class="primary" onclick={() => choose(true)}>Start fresh</button>
    </div>
  </div>
</dialog>

<style>
  dialog {
    width: min(380px, calc(100vw - 32px));
  }

  .body {
    padding: var(--pad);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  h2 {
    font-size: 1.25rem;
  }

  p {
    margin: 0;
  }

  .small {
    font-size: 0.85rem;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 0;
    border-top: var(--hairline);
    border-bottom: var(--hairline);
  }

  .buttons {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
</style>
