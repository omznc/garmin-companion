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

export type BackgroundDeps = {
  signedIn: () => Promise<boolean>;
  sample: () => boolean;
  historyPending: () => boolean;
  lastAt: () => number | null;
  // A quick sync with the retry pause of the app. True when it did not fail.
  quickSync: () => Promise<boolean>;
};

// One run of the background task. False means the run failed.
// The core takes `sync.lock` in sync_all, so a run in the app and this run never overlap.
export async function runBackground(deps: BackgroundDeps, now = Date.now()): Promise<boolean> {
  const go = shouldRunBackground({
    signedIn: await deps.signedIn(),
    sample: deps.sample(),
    historyPending: deps.historyPending(),
    lastAt: deps.lastAt(),
    now,
  });
  return go ? deps.quickSync() : true;
}
