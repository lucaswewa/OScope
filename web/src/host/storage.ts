// What the app keeps on this computer, in the browser's own storage. W1.4
// puts the platform's other doors beside it, and the desktop app's behind them.

/** What was kept under `key`, or nothing. A browser that refuses has kept nothing. */
export function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Keeps `value` under `key`. A browser that refuses, as a private window may, keeps nothing. */
export function write(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // What couldn't be kept still holds for as long as the page does.
  }
}
