// "3 days ago" for saved recipes: short and rounded, like a file list.

/** How long ago `then` (seconds since 1970) was, seen at `now` (the same). */
export function ago(then: number, now = Date.now() / 1000): string {
  const s = Math.max(0, now - then);
  const unit = (n: number, one: string) => `${n} ${one}${n === 1 ? '' : 's'} ago`;
  if (s < 60) return 'just now';
  if (s < 3600) return unit(Math.floor(s / 60), 'minute');
  if (s < 86400) return unit(Math.floor(s / 3600), 'hour');
  if (s < 86400 * 30) return unit(Math.floor(s / 86400), 'day');
  if (s < 86400 * 365) return unit(Math.floor(s / (86400 * 30)), 'month');
  return unit(Math.floor(s / (86400 * 365)), 'year');
}

/** A recipe file's name without `.recipe.toml`. */
export const recipeTitle = (name: string) => name.replace(/\.recipe\.toml$/i, '');
