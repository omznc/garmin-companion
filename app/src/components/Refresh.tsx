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
 *
 * It is also what makes a screen refreshable by pulling its top down on a
 * phone: while this is mounted its action is published to `lib/refreshable`,
 * which is the only thing the gesture in the shell knows about the screen it is
 * sitting on. See the note there for why it's registered rather than listed.
 */
import { useCallback, useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { runSync } from "../lib/syncProgress";
import { registerRefresh } from "../lib/refreshable";
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

  // Returns the work rather than firing and forgetting, because the pull
  // gesture holds its spinner down for exactly as long as this takes — and
  // swallows its own failure, because the failure is already reported in the
  // label below and a rejection nobody catches is a console error per pull.
  const run = useCallback(() => {
    if (sync.running) return Promise.resolve();
    setFailed(false);
    const work = live ? Promise.resolve() : runSync(days, false);
    return work.then(() => qc.invalidateQueries()).catch(() => setFailed(true));
  }, [sync.running, live, days, qc]);

  useEffect(() => registerRefresh(run), [run]);

  return (
    <button
      className="quiet"
      onClick={() => void run()}
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
