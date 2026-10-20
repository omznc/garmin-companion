import { useCallback, useSyncExternalStore } from "react";
import { clearHistoryPending, markHistoryPending, runSync } from "./account";
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
  // The history import is a chain of two passes. It stays true after an error, until a retry ends.
  importing: boolean;
  // True while the full pass runs.
  full: boolean;
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
  full: false,
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
    full: args.full === true,
  });
  const stop = subscribe((event) => {
    if (event.kind !== "sync-progress") return;
    const p = event.payload as SyncProgress;
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
      set({ busy: false, full: false });
      queryClient.invalidateQueries();
    });
  return inflight;
}

// Fill Today first with the last 14 days, then take the full history. The error goes in the store.
export async function importHistory(quick = true) {
  set({ importing: true });
  try {
    if (quick) await startSync({ days: 14 });
    markHistoryPending();
    await startSync({ full: true, days: 365 });
    clearHistoryPending();
    set({ importing: false });
  } catch {
    // The store holds the error and Today offers a retry.
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
