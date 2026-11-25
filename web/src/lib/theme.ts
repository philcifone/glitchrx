// Light, dark, or the system's (CSS prefers-color-scheme). A choice is
// stored per browser and set on <html>; System clears it.

export type Theme = 'dark' | 'light';
export type ThemeChoice = Theme | 'system';
const KEY = 'glitchrx.theme';

/** Bar color per theme (--card), for the browser and iOS status bar. */
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


/** The saved choice, or system. */
export function themeChoice(): ThemeChoice {
  const set = document.documentElement.dataset.theme;
  return set === 'dark' || set === 'light' ? set : 'system';
}

/** Light, dark, or back to following the system. */
export function setTheme(choice: ThemeChoice): void {
  const root = document.documentElement;
  try {
    if (choice === 'system') localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, choice);
  } catch {
    // Not persisted; fine.
  }
  if (choice === 'system') {
    delete root.dataset.theme;
    // Each scheme's own bar color again.
    document.querySelectorAll('meta[name="theme-color"]').forEach((m) => {
      const media = m.getAttribute('media') ?? '';
      m.setAttribute('content', BAR[media.includes('dark') ? 'dark' : 'light']);
    });
  } else {
    root.dataset.theme = choice;
    syncThemeColor(choice);
  }
}
