import { describe, expect, it } from 'vitest';
import { ResultCache } from './results';

const blob = (n: number) => new Blob([new Uint8Array(n)]);

describe('ResultCache', () => {
  it('gives back what was put', () => {
    const c = new ResultCache();
    const b = blob(10);
    c.put('a', b);
    expect(c.get('a')).toBe(b);
    expect(c.get('b')).toBeUndefined();
  });

  it('drops the least recently used past the count', () => {
    const c = new ResultCache(2);
    c.put('a', blob(1));
    c.put('b', blob(1));
    c.get('a'); // a is now newer than b
    c.put('c', blob(1));
    expect(c.get('b')).toBeUndefined();
    expect(c.get('a')).toBeDefined();
    expect(c.get('c')).toBeDefined();
  });

  it('keeps within the byte budget', () => {
    const c = new ResultCache(10, 100);
    c.put('a', blob(60));
    c.put('b', blob(60));
    expect(c.get('a')).toBeUndefined();
    expect(c.get('b')).toBeDefined();
    c.put('huge', blob(101));
    expect(c.get('huge')).toBeUndefined();
    expect(c.size).toBe(1);
  });

  it('replaces a key without counting it twice', () => {
    const c = new ResultCache(10, 100);
    c.put('a', blob(60));
    c.put('a', blob(60));
    c.put('b', blob(30));
    expect(c.size).toBe(2);
  });
});
