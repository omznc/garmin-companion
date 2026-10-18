import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { runSync } from "./account";
import { subscribe } from "./source";
import type { SyncProgress, SyncSummary } from "./types";

type Args = { days?: number; full?: boolean };

// One sync at a time for the whole app. A second call gets the run that is active.
let inflight: Promise<SyncSummary> | null = null;

export function useSync() {
  const queryClient = useQueryClient();
  const [busy, setBusy] = useState(inflight !== null);
  const [line, setLine] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = useCallback(
    (args: Args = {}) => {
      if (inflight) return inflight;
      setBusy(true);
      setError(null);
      setLine(null);
      const stop = subscribe((event) => {
        if (event.kind === "sync-progress") setLine((event.payload as SyncProgress).line);
      });
      inflight = runSync(args)
        .then((summary) => {
          setLine(summary.line);
          return summary;
        })
        .catch((e: unknown) => {
          setError(e instanceof Error ? e.message : String(e));
          throw e;
        })
        .finally(() => {
          stop();
          inflight = null;
          setBusy(false);
          queryClient.invalidateQueries();
        });
      return inflight;
    },
    [queryClient],
  );

  // For pull-to-refresh: the error shows in `error`, so the caller needs no catch.
  const refresh = useCallback(
    () =>
      run().then(
        () => {},
        () => {},
      ),
    [run],
  );

  return { run, refresh, busy, line, error };
}
