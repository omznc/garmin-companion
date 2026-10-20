import { useCallback, useSyncExternalStore } from "react";
import { clearHistoryPending, markHistoryPending, runSync } from "./account";
import { QUICK_DAYS } from "./importLabel";
import { queryClient } from "./queryClient";
import { subscribe } from "./source";
import type { SyncProgress, SyncSummary } from "./types";

type Args = { days?: number; full?: boolean };

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

// One sync at a time for the whole app. A second call gets the run that is active.
let inflight: Promise<SyncSummary> | null = null;

export function startSync(args: Args = {}): Promise<SyncSummary> {
  if (inflight) return inflight;
  set({
    busy: true,
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
    if (state.phase !== null && p.phase !== state.phase) queryClient.invalidateQueries();
    set({ line: p.line, phase: p.phase, done: p.done, total: p.total });
  });
  inflight = runSync(args)
    .then((summary) => {
      set({ line: summary.line });
      return summary;
    })
    .catch((e: unknown) => {
      set({ error: e instanceof Error ? e.message : String(e) });
      throw e;
    })
    .finally(() => {
      stop();
      inflight = null;
      set({ busy: false });
      queryClient.invalidateQueries();
    });
  return inflight;
}

// startSync joins a run that is active, for example a pull-to-refresh. A pass of the import must run itself.
async function ownSync(args: Args): Promise<SyncSummary> {
  while (inflight) await inflight.catch(() => {});
  return startSync(args);
}

// Fill Today first with the last 14 days, then take the full history. The error goes in the store.
export async function importHistory() {
  set({ importing: true, importError: null, importPass: "quick" });
  try {
    // The marker comes first, so a stop in the quick pass still resumes at the next start.
    markHistoryPending();
    await ownSync({ days: QUICK_DAYS });
    set({ importPass: "full" });
    await ownSync({ full: true, days: 365 });
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
        () => {},
        () => {},
      ),
    [],
  );

  return { ...s, run: startSync, refresh };
}
