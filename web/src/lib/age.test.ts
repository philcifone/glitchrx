import { describe, expect, it } from 'vitest';
import { ago, recipeTitle } from './age';

describe('ages and titles', () => {
  it('rounds down to the largest unit', () => {
    const now = 1_000_000_000;
    expect(ago(now - 5, now)).toBe('just now');
    expect(ago(now - 60, now)).toBe('1 minute ago');
    expect(ago(now - 7200, now)).toBe('2 hours ago');
    expect(ago(now - 86400 * 3, now)).toBe('3 days ago');
    expect(ago(now - 86400 * 400, now)).toBe('1 year ago');
    expect(ago(now + 50, now)).toBe('just now');
  });

  it('drops the recipe ending', () => {
    expect(recipeTitle('beach.recipe.toml')).toBe('beach');
    expect(recipeTitle('notes.toml')).toBe('notes.toml');
  });
});
