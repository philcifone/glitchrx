<script lang="ts">
  // Edit mode for the step stack (SPEC 9.5, Phase 5a), as in iOS Mail or
  // Reminders: every step on one line, a circle to select it on the left,
  // a grip to drag it on the right. Selected steps can be duplicated or
  // removed together. A preset group keeps its panel and has a grip of its
  // own, so it drags as a block; its steps drag only inside it.
  //
  // The step held to start Edit mode comes in selected: its row draws a
  // green ring inside its edge and sinks a little to make room for it, as
  // every selected row does (Phil, 2026-11-25). Rows keep their size.
  //
  // Holding a row (anywhere but its grip) leaves Edit mode, as holding a
  // card started it. The bar at the bottom is icons only; holding one shows
  // its name. Its dice re-rolls the selected steps, or all of them.
  //
  // Dragging reuses the arrow buttons' moves (app.moveStep / moveGroup),
  // so it obeys the same rules about groups. The row swaps with its
  // neighbour once the finger passes the neighbour's middle; measuring
  // against the neighbour (not the row) keeps a tall group from making the
  // row bounce back and forth. animate:flip slides the rows into place.
  import { flip } from 'svelte/animate';
  import GripVertical from '@lucide/svelte/icons/grip-vertical';
  import Circle from '@lucide/svelte/icons/circle';
  import Square from '@lucide/svelte/icons/square';
  import SquareCheck from '@lucide/svelte/icons/square-check';
  import SquareMinus from '@lucide/svelte/icons/square-minus';
  import Dices from '@lucide/svelte/icons/dices';
  import CircleCheck from '@lucide/svelte/icons/circle-check';
  import Copy from '@lucide/svelte/icons/copy';
  import Trash2 from '@lucide/svelte/icons/trash-2';
  import { app } from '../lib/app.svelte';
  import { blocksOf } from '../lib/groups';
  import { iconFor } from '../lib/icons';
  import { hold, tip } from '../lib/hold';
  import type { StepState } from '../lib/types';

  const blocks = $derived(blocksOf(app.recipe.steps, app.recipe.groups));
  let selected = $state<string[]>([]);
  // Drop selections whose steps have gone (undo, say).
  const live = $derived(selected.filter((u) => app.recipe.steps.some((s) => s.uid === u)));
  const allSelected = $derived(live.length > 0 && live.length === app.recipe.steps.length);

  const isSel = (uid: string) => live.includes(uid);
  function toggle(uid: string) {
    selected = isSel(uid) ? live.filter((u) => u !== uid) : [...live, uid];
  }
  function toggleGroup(uids: string[]) {
    const all = uids.every(isSel);
    selected = all ? live.filter((u) => !uids.includes(u)) : [...new Set([...live, ...uids])];
  }
  // Select the held step a frame after the list appears, so its ring
  // draws in rather than being there from the start.
  $effect(() => {
    const first = app.editFirst;
    app.editFirst = null;
    if (!first) return;
    const id = requestAnimationFrame(() => {
      if (!isSel(first)) selected = [...live, first];
    });
    return () => cancelAnimationFrame(id);
  });

  function selectAll() {
    selected = allSelected ? [] : app.recipe.steps.map((s) => s.uid);
  }

  function remove() {
    app.removeSteps(live);
    selected = [];
  }
  function duplicate() {
    app.duplicateSteps(live);
  }
  /** The selected steps, or with none selected, every step. */
  const targets = $derived(live.length ? live : undefined);

  // ---------------------------------------------------------------------
  // Dragging

  let list: HTMLOListElement;
  let drag = $state<{ kind: 'step' | 'group'; id: string; y: number; moved: boolean } | null>(null);
  let raf = 0;

  // The finger is followed on window, not on the grip: when a row swaps,
  // its element moves in the DOM, and a moved element loses its pointer
  // capture, so the grip would never hear the finger lift and the drag
  // would carry on by itself.
  let pointer = -1;

  function grab(e: PointerEvent, kind: 'step' | 'group', id: string) {
    e.preventDefault();
    release();
    pointer = e.pointerId;
    drag = { kind, id, y: e.clientY, moved: false };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', release);
    window.addEventListener('pointercancel', release);
    raf = requestAnimationFrame(tick);
  }

  function move(e: PointerEvent) {
    if (drag && e.pointerId === pointer) drag.y = e.clientY;
  }

  function release(e?: PointerEvent) {
    if (e && e.pointerId !== pointer) return;
    window.removeEventListener('pointermove', move);
    window.removeEventListener('pointerup', release);
    window.removeEventListener('pointercancel', release);
    cancelAnimationFrame(raf);
    if (drag?.moved) app.commit();
    drag = null;
  }

  /** The element that moves: a row, or a group's panel. */
  const dragged = () => (drag ? list.querySelector<HTMLElement>(`[data-drag="${drag.kind}:${drag.id}"]`) : null);

  // Each frame: scroll near the edges, then swap past a neighbour whose
  // middle the finger has crossed.
  function tick() {
    if (!drag) return;
    const box = list.closest<HTMLElement>('.stack');
    if (box) {
      const r = box.getBoundingClientRect();
      const edge = 56;
      if (drag.y < r.top + edge) box.scrollTop -= Math.ceil((r.top + edge - drag.y) / 6);
      else if (drag.y > r.bottom - edge) box.scrollTop += Math.ceil((drag.y - (r.bottom - edge)) / 6);
    }
    const el = dragged();
    if (el) {
      // Layout positions (offsetTop), not on-screen ones: flip's transforms
      // move rows on screen while they slide.
      const y = drag.y - list.getBoundingClientRect().top;
      const mid = (n: Element | null) => (n instanceof HTMLElement ? n.offsetTop + n.offsetHeight / 2 : null);
      const above = mid(el.previousElementSibling);
      const below = mid(el.nextElementSibling);
      const by = above !== null && y < above ? -1 : below !== null && y > below ? 1 : 0;
      if (by) {
        const ok = drag.kind === 'step' ? app.moveStep(drag.id, by, false) : app.moveGroup(drag.id, by, false);
        if (ok) drag.moved = true;
      }
    }
    raf = requestAnimationFrame(tick);
  }

  const name = (s: StepState) => app.stepName(s);

  // Leaving Edit mode mid-drag: stop listening and keep what moved.
  $effect(() => () => release());
</script>

{#snippet row(step: StepState)}
  {@const Icon = iconFor(step.effect)}
  <div
    class="row"
    class:off={!step.enabled}
    class:sel={isSel(step.uid)}
    class:lifted={drag?.kind === 'step' && drag.id === step.uid}
    {@attach hold(() => app.stopEditingSteps(), { ignore: '.grip' })}
  >
    <button class="pick" onclick={() => toggle(step.uid)} role="checkbox" aria-checked={isSel(step.uid)} aria-label="Select {name(step)}">
      {#if isSel(step.uid)}<CircleCheck size={22} strokeWidth={1.75} />{:else}<Circle size={22} strokeWidth={1.5} />{/if}
    </button>
    <button class="what" onclick={() => toggle(step.uid)} tabindex="-1">
      <Icon size={17} strokeWidth={1.75} />
      <span class="serif">{step.effect}</span>{#if app.stepNumber(step)}<span class="num">-{app.stepNumber(step)}</span>{/if}
      {#if !step.enabled}<span class="dim tag">off</span>{/if}
    </button>
    <span
      class="grip"
      role="button"
      tabindex="-1"
      aria-label="Drag {name(step)}"
      onpointerdown={(e) => grab(e, 'step', step.uid)}
    >
      <GripVertical size={20} strokeWidth={1.75} />
    </span>
  </div>
{/snippet}

<ol class="edit" bind:this={list} class:dragging={!!drag}>
  {#each blocks as b (b.kind === 'step' ? b.step.uid : b.group.id)}
    <li
      animate:flip={{ duration: 160 }}
      data-drag={b.kind === 'step' ? `step:${b.step.uid}` : `group:${b.group.id}`}
      class={b.kind === 'group' ? 'group' : 'loose'}
      class:lifted={b.kind === 'group' && drag?.kind === 'group' && drag.id === b.group.id}
    >
      {#if b.kind === 'step'}
        {@render row(b.step)}
      {:else}
        {@const uids = b.steps.map((s) => s.step.uid)}
        <div class="ghead">
          <button class="pick" onclick={() => toggleGroup(uids)} role="checkbox" aria-checked={uids.every(isSel)} aria-label="Select the group {b.group.preset}">
            {#if uids.every(isSel)}<CircleCheck size={22} strokeWidth={1.75} />{:else}<Circle size={22} strokeWidth={1.5} />{/if}
          </button>
          <span class="from">From preset</span>
          <span class="serif pname">{b.group.preset}</span>
          <span
            class="grip"
            role="button"
            tabindex="-1"
            aria-label="Drag the group {b.group.preset}"
            onpointerdown={(e) => grab(e, 'group', b.group.id)}
          >
            <GripVertical size={20} strokeWidth={1.75} />
          </span>
        </div>
        <ol class="inner">
          {#each b.steps as s (s.step.uid)}
            <li animate:flip={{ duration: 160 }} data-drag="step:{s.step.uid}">{@render row(s.step)}</li>
          {/each}
        </ol>
      {/if}
    </li>
  {/each}
</ol>

<div class="bar" role="toolbar" aria-label="Selected steps">
  <button
    class="icon"
    {@attach tip}
    onclick={selectAll}
    disabled={!app.recipe.steps.length}
    role="checkbox"
    aria-checked={allSelected ? 'true' : live.length ? 'mixed' : 'false'}
    title={allSelected ? 'Select none' : 'Select all'}
    aria-label={allSelected ? 'Select none' : 'Select all'}
  >
    {#if allSelected}
      <SquareCheck size={22} strokeWidth={1.75} />
    {:else if live.length}
      <SquareMinus size={22} strokeWidth={1.75} />
    {:else}
      <Square size={22} strokeWidth={1.75} />
    {/if}
  </button>
  <span class="dim count">{live.length ? `${live.length} selected` : ''}</span>
  <button
    class="icon"
    {@attach tip}
    onclick={() => app.rerollSteps(targets)}
    class:dimmed={!app.anySeededIn(targets)}
    aria-disabled={!app.anySeededIn(targets)}
    title={live.length ? 'New seeds for the selected steps' : 'New seeds for every step'}
    aria-label={live.length ? 'New seeds for the selected steps' : 'New seeds for every step'}
  >
    <Dices size={20} strokeWidth={1.75} />
  </button>
  <button class="icon" {@attach tip} onclick={duplicate} disabled={!live.length} title="Duplicate the selected steps (new seeds)" aria-label="Duplicate the selected steps">
    <Copy size={19} strokeWidth={1.75} />
  </button>
  <button class="icon danger" {@attach tip} onclick={remove} disabled={!live.length} title="Remove the selected steps (undo brings them back)" aria-label="Remove the selected steps">
    <Trash2 size={19} strokeWidth={1.75} />
  </button>
</div>

<style>
  .edit {
    list-style: none;
    margin: 0;
    padding: 0;
    position: relative; /* rows measure their offsetTop against it */
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .edit.dragging {
    user-select: none;
    -webkit-user-select: none;
  }

  .inner {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--card);
    border-radius: 12px;
    min-height: 48px;
    padding: 0 4px;
    transition:
      box-shadow 0.22s var(--ease-out, cubic-bezier(0.32, 0.72, 0, 1)),
      transform 0.22s var(--ease-out, cubic-bezier(0.32, 0.72, 0, 1));
  }

  /* Selected: a green ring inside the edge, and the row sinks a little,
     as iOS does when it picks up an item. Neither changes the layout. */
  .row.sel {
    box-shadow: inset 0 0 0 2.5px var(--accent);
    transform: scale(0.975);
  }

  .row.off .what {
    opacity: 0.5;
  }

  .row.lifted,
  .group.lifted {
    box-shadow: 0 6px 18px rgb(0 0 0 / 0.18);
    transform: scale(1.02);
    position: relative;
    z-index: 1;
  }

  /* Picked up while selected: keep the ring. */
  .row.sel.lifted {
    box-shadow:
      inset 0 0 0 2.5px var(--accent),
      0 6px 18px rgb(0 0 0 / 0.18);
  }

  .pick {
    flex: none;
    background: none;
    padding: 0 6px;
    min-height: 44px;
    color: var(--accent);
  }

  .what {
    flex: 1;
    min-width: 0;
    justify-content: flex-start;
    gap: 8px;
    background: none;
    padding: 0 4px;
    min-height: 44px;
    font-weight: 400;
  }

  .what :global(svg) {
    color: var(--accent);
    flex: none;
  }

  .what .serif {
    font-size: 1rem;
  }

  .num {
    margin-left: -8px;
    font-variant-numeric: lining-nums tabular-nums;
    font-size: 0.9rem;
  }

  .tag {
    font-size: 0.75rem;
  }

  /* The grip takes the touch for itself, so dragging it never scrolls. */
  .grip {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    min-height: 44px;
    color: var(--fg-dim);
    touch-action: none;
    cursor: grab;
  }

  .group {
    background: var(--fill);
    border-radius: calc(var(--radius-lg) + 4px);
    padding: 2px 6px 6px;
  }

  .ghead {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .from {
    flex: none;
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-dim);
  }

  .pname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bar {
    position: sticky;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 4px calc(8px + env(safe-area-inset-bottom));
    margin-top: auto;
    background: var(--bg);
    border-top: var(--hairline);
    z-index: 2;
  }

  /* Looks disabled but still answers a tap or a hold, to say why
     (nothing random in these steps): a disabled button gets no touches. */
  .dimmed {
    opacity: 0.35;
  }

  .count {
    flex: 1;
    text-align: center;
    font-size: 0.85rem;
  }

  /* Held rows leave Edit mode: no text selection or system menu. */
  .row {
    -webkit-user-select: none;
    user-select: none;
    -webkit-touch-callout: none;
  }
</style>
