import { useCallback, useSyncExternalStore } from "react";
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
import { createSyncQueue, type SyncArgs } from "./syncQueue";
import type { SyncProgress, SyncSummary } from "./types";

type Args = SyncArgs;

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
const SYNC_KEYS = ["today", "activities", "activity", "body", "health", "sleep", "settings"];
const refetchSyncData = () =>
  queryClient.invalidateQueries({ predicate: (q) => SYNC_KEYS.includes(String(q.queryKey[0])) });

function runOnce(args: Args): Promise<SyncSummary> {
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
  return runSync(args)
    .then((summary) => {
      markSynced();
      set({ line: summary.line });
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
export async function syncIfStale(): Promise<void> {
  if (state.busy || historyPending()) return;
  const status = await getStatus().catch(() => null);
  if (!status?.connected || isSampleData()) return;
  if (!syncIsStale(lastSyncedAt(), Date.now())) return;
  await startSync({ days: QUICK_DAYS }).catch(() => {});
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
