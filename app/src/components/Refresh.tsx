/**
 * The per-screen refresh control.
 *
 * Worth being precise about what this refreshes. Every paragraph and derived
 * figure in this app is a pure function of the local cache, recomputed on each
 * render — so a button that only refetched the queries would be honest and
 * visibly do nothing. The thing that can actually be out of date is the cache
 * itself, which means refresh has to mean *sync*, then invalidate.
 *
 * State is shared with the sidebar's Sync button rather than owned here:
 * `runSync` collapses concurrent callers into one pass, so pressing this while
 * a sync is already running joins it instead of queueing a second walk over the
 * same days — and both controls have to show that.
 */
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { runSync } from "../lib/syncProgress";
import { useSyncState } from "./SyncBar";
import { RotateArrow } from "./ui";

export function RefreshButton({
  days = 30,
  label = "Refresh",
  live = false,
}: {
  /**
   * How far back to look, not how far back to re-fetch: a sync re-asks about
   * the last few days and then only about days the cache is missing inside
   * this window. The default matches the sidebar's Sync.
   */
  days?: number;
  label?: string;
  /**
   * Set on screens that call Garmin directly rather than reading the cache —
   * Gear is the one. Syncing wouldn't touch what they show, so those refetch
   * and skip the sync entirely.
   */
  live?: boolean;
}) {
  const qc = useQueryClient();
  const sync = useSyncState();
  const [failed, setFailed] = useState(false);

  const run = () => {
    if (sync.running) return;
    setFailed(false);
    const work = live ? Promise.resolve() : runSync(days, false);
    work.then(() => qc.invalidateQueries()).catch(() => setFailed(true));
  };

  return (
    <button
      className="quiet"
      onClick={run}
      disabled={sync.running}
      title={
        failed
          ? "The last sync failed — Settings has the detail"
          : live
            ? "Ask Garmin for this again"
            : "Pull anything new from Garmin and rebuild this screen"
      }
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        fontSize: "var(--fs-caption)",
        letterSpacing: "0.02em",
        // The failure is stated in the label rather than coloured into it —
        // an accent here would compete with the accents that mean "your
        // training needs attention", which is the one thing accent means.
        color: sync.running ? "var(--faint)" : "var(--mut)",
        cursor: sync.running ? "default" : "pointer",
        flex: "none",
      }}
    >
      <RotateArrow spinning={sync.running} />
      {sync.running ? "Syncing…" : failed ? "Retry sync" : label}
    </button>
  );
}
