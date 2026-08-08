/**
 * Where a running sync has got to.
 *
 * A first sync of a watch worn for a year is one request per endpoint per day
 * — minutes of nothing, with no way to tell work from a hang. The backend emits
 * a step per date; this holds the latest one so anything on screen can say what
 * is happening and which day it's on.
 *
 * A module store rather than React state because two screens (the sidebar
 * button and Settings) start syncs and a third displays them, and the display
 * has to survive navigating between them.
 */
import { listen } from "@tauri-apps/api/event";
import { scheduleNudges, syncNow, type SyncReport } from "./api";

export interface SyncStep {
  /** `activities`, `wellness`, `workouts`, `tracks`, `done`. */
  phase: string;
  /** The date being fetched, where the phase works by date. */
  detail: string;
  done: number;
  /** Absent for phases that can't know their length up front. */
  total: number | null;
}

export interface SyncState {
  running: boolean;
  /** True for a full re-sync, which is the long one worth narrating. */
  full: boolean;
  step: SyncStep | null;
  startedAt: number | null;
  error: string | null;
}

let state: SyncState = { running: false, full: false, step: null, startedAt: null, error: null };
const listeners = new Set<() => void>();

function set(next: Partial<SyncState>) {
  state = { ...state, ...next };
  listeners.forEach((f) => f());
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function getSyncState(): SyncState {
  return state;
}

// Subscribed once, for the life of the process. Events that arrive between
// syncs are harmless: `running` gates everything that reads `step`.
void listen<SyncStep>("sync:progress", (e) => set({ step: e.payload }));

let inFlight: Promise<SyncReport> | null = null;

/**
 * Run a sync, with progress. Concurrent callers share one run rather than
 * queueing a second pass over the same days.
 */
export function runSync(days: number, full: boolean): Promise<SyncReport> {
  if (inFlight) return inFlight;
  set({ running: true, full, step: null, startedAt: Date.now(), error: null });
  inFlight = syncNow(days, full)
    .then((report) => {
      // New data means the rules can have changed their mind, and what the
      // system has queued was built from the old answer. This and app launch
      // are the only two moments the plan can be rebuilt — nothing evaluates
      // the coach while the app is closed — so a sync must not pass without it.
      void scheduleNudges().catch(() => {
        // Notifications being refused or unavailable is not a failed sync.
      });
      return report;
    })
    .catch((e: unknown) => {
      const message = e instanceof Error ? e.message : String(e);
      set({ error: message });
      throw e;
    })
    .finally(() => {
      inFlight = null;
      set({ running: false, step: null, startedAt: null });
    });
  return inFlight;
}

/** One line of plain English for a step. */
export function describe(step: SyncStep | null): { title: string; detail: string } {
  if (!step) return { title: "Starting", detail: "Asking Garmin what's new" };
  switch (step.phase) {
    case "activities":
      return {
        title: "Activities",
        detail: step.detail ? `${step.done} so far, back to ${step.detail}` : `${step.done} so far`,
      };
    case "wellness":
      return {
        title: "Daily health",
        // The date is the point: sleep, HRV, resting HR and readiness are all
        // fetched per day, so the day on screen is the progress.
        detail: step.detail
          ? `Sleep, HRV and readiness for ${step.detail}`
          : "Sleep, HRV and readiness",
      };
    case "workouts":
      return { title: "Workouts", detail: "Your saved workouts" };
    case "tracks":
      return {
        title: "Routes",
        detail: step.total ? `GPS trace ${step.done + 1} of ${step.total}` : "GPS traces",
      };
    default:
      return { title: "Finishing", detail: "Writing to the cache" };
  }
}

/** 0–1, or null where the phase can't know how far it has to go. */
export function fraction(step: SyncStep | null): number | null {
  if (!step?.total) return null;
  return Math.min(1, step.done / step.total);
}
