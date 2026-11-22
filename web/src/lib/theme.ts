// Dark/light toggle. With no choice saved, the system setting applies (CSS
// prefers-color-scheme); a choice is stored per browser and set on <html>.

export type Theme = 'dark' | 'light';
const KEY = 'glitchrx.theme';

/** Bar colour per theme (--card), for the browser and iOS status bar. */
const BAR: Record<Theme, string> = { light: '#fbf7ec', dark: '#282828' };

/**
 * index.html has one theme-color per system scheme. When the toggle forces
 * a theme, point both at it, so the status bar matches the app.
 */
function syncThemeColor(t: Theme): void {
  document.querySelectorAll('meta[name="theme-color"]').forEach((m) => m.setAttribute('content', BAR[t]));
}

export function currentTheme(): Theme {
  const set = document.documentElement.dataset.theme;
  if (set === 'dark' || set === 'light') return set;
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function applySavedTheme(): void {
  try {
    const t = localStorage.getItem(KEY);
    if (t === 'dark' || t === 'light') {
      document.documentElement.dataset.theme = t;
      syncThemeColor(t);
    }
  } catch {
    // Storage blocked: follow the system.
  }
}

export function toggleTheme(): Theme {
  const next: Theme = currentTheme() === 'dark' ? 'light' : 'dark';
  document.documentElement.dataset.theme = next;
  syncThemeColor(next);
  try {
    localStorage.setItem(KEY, next);
  } catch {
    // Not persisted; fine.
  }
  return next;
}
