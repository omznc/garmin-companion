import { useCallback, useSyncExternalStore } from "react";
import { AppState } from "react-native";
import { SyncService } from "../../modules/sync-service";
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

// An import that the system cut off in the background shows no error and runs again in the
// foreground. A manual sync that the system cut off shows the short offline message.
type CutOff = { cutOff?: boolean };
const isCutOff = (e: unknown) => (e as CutOff | null)?.cutOff === true;
let inImport = false;
let importCut = false;

// Called when the app comes back to the foreground.
export function resumeCutImport() {
  if (!importCut || state.importing) return;
  importCut = false;
  void importHistory();
}

async function runTracked(args: SyncArgs): Promise<SyncSummary> {
  const states: string[] = [String(AppState.currentState)];
  const sub = AppState.addEventListener("change", (s) => states.push(s));
  try {
    return await runSync(args);
  } catch (e) {
    if (shouldRetryOffline(e, states)) Object.assign(e as object, { cutOff: true });
    throw e;
  } finally {
    sub.remove();
  }
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
  return runTracked(args)
    .then(async (summary) => {
      markSynced();
      set({ line: summary.line });
      // The background task waits for the notification before it ends.
      await notifySafely(notifyDeps, args.full === true);
      return summary;
    })
    .catch((e: unknown) => {
      if (!(inImport && isCutOff(e))) set({ error: e instanceof Error ? e.message : String(e) });
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
  } catch (e) {
    // A cut-off run is not a Garmin fault, so the next return to the foreground may sync at once.
    if (!isCutOff(e)) failedAt = Date.now();
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

// The import runs without the service if the system refuses it or the native call throws.
function startService(): boolean {
  try {
    return AppState.currentState === "active" && SyncService?.start() === true;
  } catch {
    return false;
  }
}

// Fill Today first with the last 14 days, then take the full history. The error goes in the store.
// On Android a foreground service keeps the network open when the person leaves the app. The system
// refuses to start it in the background. Then a cut import resumes in the foreground (resumeCutImport).
export async function importHistory() {
  // Sign-in and the start of the app can both ask. One import at a time owns the service.
  if (state.importing) return;
  set({ importing: true, importError: null, importPass: "quick" });
  inImport = true;
  importCut = false;
  const service = startService();
  try {
    // The marker comes first, so a stop in the quick pass still resumes at the next start.
    markHistoryPending();
    await startSync({ days: QUICK_DAYS });
    set({ importPass: "full" });
    await startSync({ full: true, days: 365 });
    clearHistoryPending();
    inImport = false;
    set({ importing: false, importPass: null });
  } catch (e) {
    inImport = false;
    importCut = isCutOff(e);
    // Today offers a retry.
    set({
      importing: false,
      importPass: null,
      importError: importCut ? null : e instanceof Error ? e.message : String(e),
    });
    // The failure can arrive after the return to the foreground. The pause gives the system
    // time to open the network again. The resumed import starts its own service. If the app left
    // again in the pause, the next return to the foreground resumes it.
    if (importCut && AppState.currentState === "active") {
      setTimeout(() => AppState.currentState === "active" && resumeCutImport(), 1000);
    }
  } finally {
    if (service) SyncService?.stop();
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
