/**
 * Redraw when a sync nobody asked for finishes.
 *
 * The desktop build syncs on its own schedule — see `background.rs` — and that
 * sync can land while the window is open and being read. Every screen is drawn
 * from cache reads that only refetch on focus or after thirty seconds of
 * staleness, so without this the app would sit there showing the numbers the
 * sync just replaced, on a window that never lost focus and so never asks again.
 *
 * A module-level listener rather than a hook, for the same reason as
 * `dayRollover`: one for the life of the process, not one per mount.
 */
import { listen } from "@tauri-apps/api/event";
import type { QueryClient } from "@tanstack/react-query";

export function startBackgroundRefresh(qc: QueryClient): void {
  // Everything, not a subset: a sync writes activities, daily wellness,
  // workouts and routes, and the derived screens on top of them share no query
  // key that says which of those they read.
  void listen("background:synced", () => {
    void qc.invalidateQueries();
  });
}
