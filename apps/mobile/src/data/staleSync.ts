// When the app opens, it starts a quick sync if the last good sync is old.
export const STALE_AFTER_MS = 3 * 60 * 60 * 1000;

// A missing time (never synced here) and a time in the future (the clock moved) both read as stale.
export function syncIsStale(lastAt: number | null, now: number): boolean {
  if (lastAt === null || lastAt > now) return true;
  return now - lastAt >= STALE_AFTER_MS;
}
