// localStorage can throw (private mode, blocked storage); never let it
// break the app. Everything kept here is a per-browser convenience.

export function storageGet(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

export function storageSet(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Persistence is a convenience only.
  }
}
