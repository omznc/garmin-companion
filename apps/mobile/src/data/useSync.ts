import { useCallback, useSyncExternalStore } from "react";
import { AppState } from "react-native";
import { notifyDeps } from "../notify/notifications";
import { notifySafely } from "../notify/run";
import { committed } from "../ui/haptics";
import {
  clearHistoryPending,
  getStatus,
  historyPending,
  lastSyncedAt,
  markHistoryPending,
  markSynced,
  runSync,
} from "./account";
import { QUICK_DAYS } from "./importLabel";
import { queryClient } from "./queryClient";
import { isSampleData, subscribe } from "./source";
import { syncIsStale } from "./staleSync";
import { shouldRetryOffline } from "./syncRetry";
import { createSyncQueue, type SyncArgs } from "./syncQueue";
import type { SyncProgress, SyncSummary } from "./types";

export type SyncState = {
  busy: boolean;
  line: string | null;
  error: string | null;
  phase: string | null;
  done: number;
  total: number | null;
  // The history import is a chain of two passes. Only importHistory sets these three fields,
  // so a pull-to-refresh can not change the import row.
  importing: boolean;
  importPass: "quick" | "full" | null;
  importError: string | null;
};

// The state is outside React, so sign-in can start a sync and Today can show it.
let state: SyncState = {
  busy: false,
  line: null,
  error: null,
  phase: null,
  done: 0,
  total: null,
  importing: false,
  importPass: null,
  importError: null,
};
const listeners = new Set<() => void>();
const set = (patch: Partial<SyncState>) => {
  state = { ...state, ...patch };
  listeners.forEach((l) => l());
};
const watch = (l: () => void) => {
  listeners.add(l);
  return () => listeners.delete(l);
};

// The query keys that a sync can change. Threads and the cloud account do not change.
const SYNC_KEYS = [
  "today",
  "activities",
  "activity",
  "body",
  "health",
  "sleep",
  "tiles",
  "extras",
  "charts",
  "settings",
];
const refetchSyncData = () =>
  queryClient.invalidateQueries({ predicate: (q) => SYNC_KEYS.includes(String(q.queryKey[0])) });

const untilActive = () =>
  new Promise<void>((resolve) => {
    if (AppState.currentState === "active") return resolve();
    const sub = AppState.addEventListener("change", (s) => {
      if (s !== "active") return;
      sub.remove();
      resolve();
    });
  });

// Run the sync. If the system blocked the network in the background, wait for the foreground and run it
// once more. The promise ends only when the sync really ended, so a caller can await it.
async function runWithRetry(args: SyncArgs): Promise<SyncSummary> {
  const states: string[] = [String(AppState.currentState)];
  const sub = AppState.addEventListener("change", (s) => states.push(s));
  try {
    return await runSync(args);
  } catch (e) {
    if (!shouldRetryOffline(e, states)) throw e;
  } finally {
    sub.remove();
  }
  await untilActive();
  return runSync(args);
}

function runOnce(args: SyncArgs): Promise<SyncSummary> {
  set({
    error: null,
    line: null,
    phase: null,
    done: 0,
    total: null,
  });
  const stop = subscribe((event) => {
    if (event.kind !== "sync-progress") return;
    const p = event.payload as SyncProgress;
    // A finished stage shows on Today now, not after the whole pass.
    if (state.phase !== null && p.phase !== state.phase) refetchSyncData();
    set({ line: p.line, phase: p.phase, done: p.done, total: p.total });
  });
  return runWithRetry(args)
    .then(async (summary) => {
      markSynced();
      set({ line: summary.line });
      // The background task waits for the notification before it ends.
      await notifySafely(notifyDeps, args.full === true);
      return summary;
    })
    .catch((e: unknown) => {
      set({ error: e instanceof Error ? e.message : String(e) });
      throw e;
    })
    .finally(() => {
      stop();
      refetchSyncData();
    });
}

// One sync at a time for the whole app. A call with the same args joins the last run.
// A call with other args waits, so the full import is never met by a quick pull-to-refresh.
export const startSync = createSyncQueue(runOnce, (busy) => set({ busy }));

// Called when the app opens and when it comes back to the foreground. Not in sample mode, not signed out.
// An import that waits to resume starts itself, so this does not start a second sync then.
// A failed run waits for the retry pause, so a dead token or no network does not call Garmin at each
// return to the foreground. The pause lasts until the app restarts. The person sees no error then.
const RETRY_PAUSE_MS = 30 * 60 * 1000;
let failedAt: number | null = null;
const paused = (now: number) => failedAt !== null && now - failedAt < RETRY_PAUSE_MS;

// A quick sync that never throws and never leaves an error on Today. The foreground and the
// background task both use it, so both obey the same retry pause. False means the run failed.
export async function quickSync(): Promise<boolean> {
  if (paused(Date.now())) return true;
  try {
    await startSync({ days: QUICK_DAYS });
    failedAt = null;
    return true;
  } catch {
    failedAt = Date.now();
    set({ error: null });
    return false;
  }
}

export async function syncIfStale(): Promise<void> {
  if (state.busy || historyPending()) return;
  const now = Date.now();
  if (paused(now) || !syncIsStale(lastSyncedAt(), now)) return;
  const status = await getStatus().catch(() => null);
  if (!status?.connected || isSampleData()) return;
  await quickSync();
}

// Fill Today first with the last 14 days, then take the full history. The error goes in the store.
export async function importHistory() {
  set({ importing: true, importError: null, importPass: "quick" });
  try {
    // The marker comes first, so a stop in the quick pass still resumes at the next start.
    markHistoryPending();
    await startSync({ days: QUICK_DAYS });
    set({ importPass: "full" });
    await startSync({ full: true, days: 365 });
    clearHistoryPending();
    set({ importing: false, importPass: null });
  } catch (e) {
    // Today offers a retry.
    set({
      importing: false,
      importPass: null,
      importError: e instanceof Error ? e.message : String(e),
    });
  }
}

export function useSync() {
  const s = useSyncExternalStore(watch, () => state);

  const refresh = useCallback(
    () =>
      startSync().then(
        () => committed(),
        () => {},
      ),
    [],
  );

  return { ...s, run: startSync, refresh };
}
