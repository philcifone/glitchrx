// Bring an element into view by scrolling only its nearest scrolling box.
//
// Element.scrollIntoView() scrolls every scrollable ancestor, the page
// itself included. The app is one screen tall, but on iOS the page can
// still scroll a little (the toolbars change the viewport), so each call
// nudged the whole app up and left a blank band at the bottom that grew
// with every step added. This scrolls the step list and nothing else.

export function revealIn(el: Element | null | undefined, smooth = true): void {
  if (!el) return;
  let box = el.parentElement;
  while (box && !/(auto|scroll)/.test(getComputedStyle(box).overflowY)) box = box.parentElement;
  if (!box) return;
  const r = el.getBoundingClientRect();
  const b = box.getBoundingClientRect();
  // As 'nearest': down just enough to show its bottom, but never past its top.
  let dy = 0;
  if (r.bottom > b.bottom) dy = r.bottom - b.bottom;
  if (r.top - dy < b.top) dy = r.top - b.top;
  if (dy) box.scrollBy({ top: dy, behavior: smooth ? 'smooth' : 'auto' });
}
