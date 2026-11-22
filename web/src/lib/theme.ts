// Dark/light toggle. With no choice saved, the system setting applies (CSS
// prefers-color-scheme); a choice is stored per browser and set on <html>.

export type Theme = 'dark' | 'light';
const KEY = 'glitchrx.theme';

export function currentTheme(): Theme {
  const set = document.documentElement.dataset.theme;
  if (set === 'dark' || set === 'light') return set;
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function applySavedTheme(): void {
  try {
    const t = localStorage.getItem(KEY);
    if (t === 'dark' || t === 'light') document.documentElement.dataset.theme = t;
  } catch {
    // Storage blocked: follow the system.
  }
}

export function toggleTheme(): Theme {
  const next: Theme = currentTheme() === 'dark' ? 'light' : 'dark';
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem(KEY, next);
  } catch {
    // Not persisted; fine.
  }
  return next;
}
