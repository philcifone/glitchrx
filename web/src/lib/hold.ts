// Press and hold, as on iOS (SPEC 9.5): holding a step turns Edit mode on
// or off, and holding an icon-only button shows its name. Both are Svelte
// attachments (`{@attach ...}`).
//
// A hold is a press that stays put for HOLD_MS. Moving more than a few
// pixels (a scroll) or lifting early cancels it, and the click that would
// follow a hold is swallowed, so holding never also taps.

import type { Attachment } from 'svelte/attachments';

export const HOLD_MS = 450;
/** How far the finger may drift before it counts as a scroll. */
const SLOP = 10;

/**
 * Swallow the next click anywhere (capture phase), up to a moment after
 * the finger lifts. On the window, not the element: a hold often changes
 * the page (Edit mode swaps the cards for a list), and the click would
 * otherwise land on whatever is under the finger now.
 */
function swallowNextClick() {
  const eat = (e: MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    done();
  };
  const later = () => setTimeout(done, 400);
  function done() {
    window.removeEventListener('click', eat, true);
    window.removeEventListener('pointerup', later, true);
  }
  window.addEventListener('click', eat, true);
  window.addEventListener('pointerup', later, { capture: true, once: true });
}

export interface HoldOptions {
  /** Presses starting inside these (a selector) are left alone. */
  ignore?: string;
  /** Only fingers and pens: a mouse has hover titles instead. */
  touchOnly?: boolean;
  /** When the hold ends (lifted or cancelled), after it fired. */
  onend?: () => void;
}

/** Call `onhold` when the element is pressed and held still. */
export function hold(onhold: () => void, opts: HoldOptions = {}): Attachment<HTMLElement> {
  return (node) => {
    let timer = 0;
    let start: { x: number; y: number; id: number } | null = null;
    let fired = false;

    const reset = () => {
      clearTimeout(timer);
      timer = 0;
      start = null;
      node.classList.remove('pressing');
      if (fired) opts.onend?.();
      fired = false;
    };

    const down = (e: PointerEvent) => {
      if (e.button !== 0 || !e.isPrimary) return;
      if (opts.touchOnly && e.pointerType === 'mouse') return;
      if (opts.ignore && (e.target as Element | null)?.closest(opts.ignore)) return;
      reset();
      start = { x: e.clientX, y: e.clientY, id: e.pointerId };
      // A class to style the press by (a card sinks a little, as on iOS).
      node.classList.add('pressing');
      timer = window.setTimeout(() => {
        timer = 0;
        fired = true;
        node.classList.remove('pressing');
        swallowNextClick();
        onhold();
      }, HOLD_MS);
    };

    const move = (e: PointerEvent) => {
      if (start && e.pointerId === start.id && timer && Math.hypot(e.clientX - start.x, e.clientY - start.y) > SLOP) reset();
    };

    // Long presses on touch screens also open the system menu (Android) or
    // select text (iOS); not while a hold is under way.
    const menu = (e: Event) => {
      if (timer || fired) e.preventDefault();
    };

    node.addEventListener('pointerdown', down);
    node.addEventListener('pointermove', move);
    node.addEventListener('pointerup', reset);
    node.addEventListener('pointercancel', reset);
    node.addEventListener('pointerleave', reset);
    node.addEventListener('contextmenu', menu);
    return () => {
      clearTimeout(timer);
      node.removeEventListener('pointerdown', down);
      node.removeEventListener('pointermove', move);
      node.removeEventListener('pointerup', reset);
      node.removeEventListener('pointercancel', reset);
      node.removeEventListener('pointerleave', reset);
      node.removeEventListener('contextmenu', menu);
    };
  };
}

/**
 * Hold an icon-only button to see its name (its `title`, else its
 * `aria-label`) in a small bubble above it, as iOS toolbars do; a tap still
 * just taps. With a mouse the browser's own title tooltip does this.
 */
export const tip: Attachment<HTMLElement> = (node) => {
  let bubble: HTMLDivElement | null = null;
  let hide = 0;

  const remove = () => {
    clearTimeout(hide);
    bubble?.remove();
    bubble = null;
  };

  const show = () => {
    remove();
    const text = node.title || node.getAttribute('aria-label');
    if (!text) return;
    bubble = document.createElement('div');
    bubble.className = 'tip-bubble';
    bubble.setAttribute('role', 'tooltip');
    bubble.textContent = text;
    document.body.appendChild(bubble);
    // Above the button, centred on it, kept on screen.
    const r = node.getBoundingClientRect();
    const b = bubble.getBoundingClientRect();
    const left = Math.min(Math.max(8, r.left + r.width / 2 - b.width / 2), window.innerWidth - b.width - 8);
    const top = r.top - b.height - 8 >= 8 ? r.top - b.height - 8 : r.bottom + 8;
    bubble.style.left = `${left}px`;
    bubble.style.top = `${top}px`;
  };

  const detach = hold(show, {
    touchOnly: true,
    onend: () => {
      clearTimeout(hide);
      hide = window.setTimeout(remove, 1400);
    },
  })(node);

  return () => {
    remove();
    if (typeof detach === 'function') detach();
  };
};
