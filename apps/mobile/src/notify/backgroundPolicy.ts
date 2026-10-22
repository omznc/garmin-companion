// When the background task syncs. Pure, so a test can check it.

// The app has just synced: the task has nothing to add.
export const MIN_GAP_MS = 60 * 60 * 1000;

export function shouldRunBackground(f: {
  signedIn: boolean;
  sample: boolean;
  historyPending: boolean;
  lastAt: number | null;
  now: number;
}): boolean {
  if (!f.signedIn || f.sample) return false;
  // The app resumes a history import itself, and the import must not meet a quick sync.
  if (f.historyPending) return false;
  // A time in the future (the clock moved) reads as old.
  if (f.lastAt !== null && f.lastAt <= f.now && f.now - f.lastAt < MIN_GAP_MS) return false;
  return true;
}
