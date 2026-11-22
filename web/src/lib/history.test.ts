import { describe, expect, it } from 'vitest';
import { History } from './history';

describe('History', () => {
  it('undoes and redoes', () => {
    const h = new History('a');
    expect(h.canUndo).toBe(false);
    h.commit('b');
    h.commit('c');
    expect(h.undo()).toBe('b');
    expect(h.undo()).toBe('a');
    expect(h.undo()).toBeNull();
    expect(h.redo()).toBe('b');
    expect(h.present).toBe('b');
  });

  it('ignores repeats and drops the redo branch on a new commit', () => {
    const h = new History('a');
    expect(h.commit('a')).toBe(false);
    h.commit('b');
    h.undo();
    h.commit('x');
    expect(h.canRedo).toBe(false);
    expect(h.undo()).toBe('a');
  });

  it('keeps at most `limit` past states', () => {
    const h = new History('0', 3);
    for (let i = 1; i <= 10; i++) h.commit(String(i));
    let n = 0;
    while (h.undo() !== null) n++;
    expect(n).toBe(3);
    expect(h.present).toBe('7');
  });
});
