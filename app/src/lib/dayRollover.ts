/**
 * Invalidate everything when the calendar day turns over.
 *
 * Almost every derived number in this app is computed against `new Date()` at
 * render time — `dailySeries` pads to today, `dailyDistance` walks back from
 * today, `acuteChronic` slices the last 7 and 28 days, and Today's greeting and
 * weekday axis are both anchored to it. None of that re-renders on its own.
 *
 * This is a desktop app people leave open. Cross midnight with the window
 * focused and the whole screen keeps yesterday's framing: the axis ends on the
 * wrong weekday, "Today's session" describes a run from yesterday, and the load
 * ratio is a day stale. `refetchOnWindowFocus` only rescues it if you actually
 * leave and come back.
 *
 * So: a timer to the next local midnight, plus a check whenever the window
 * comes back. The check compares real dates rather than trusting the timer,
 * because a suspended laptop fires a 6-hour timeout whenever it feels like it.
 */
import type { QueryClient } from "@tanstack/react-query";
import { isoDate } from "./format";

/** Just past midnight — a timer that fires exactly on the boundary can land on
 *  the wrong side of it by a millisecond of clock skew. */
const GRACE_MS = 2_000;

function msUntilNextMidnight(now: Date): number {
  const next = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1);
  return next.getTime() - now.getTime() + GRACE_MS;
}

export function startDayRollover(qc: QueryClient): () => void {
  let today = isoDate(new Date());
  let timer: ReturnType<typeof setTimeout> | undefined;

  const check = () => {
    const now = isoDate(new Date());
    if (now === today) return;
    today = now;
    // Everything, not a key subset: the day is an input to derived values on
    // every screen, and none of them share a query key that says so.
    void qc.invalidateQueries();
  };

  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => {
      check();
      schedule();
    }, msUntilNextMidnight(new Date()));
  };

  // Suspend and resume don't fire the timer on time, and the app may have been
  // in the background for days. Both events are cheap and idempotent.
  const onWake = () => {
    check();
    schedule();
  };

  schedule();
  window.addEventListener("focus", onWake);
  document.addEventListener("visibilitychange", onWake);

  return () => {
    clearTimeout(timer);
    window.removeEventListener("focus", onWake);
    document.removeEventListener("visibilitychange", onWake);
  };
}
