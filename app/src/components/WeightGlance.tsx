/**
 * The one-line mention of body weight that Today and Food carry, and the link
 * through to the screen itself.
 *
 * It renders nothing at all when the account has no weigh-ins. That's the point
 * of putting it here rather than inlining the link twice: "are they tracking
 * weight" is one question with one answer, and a dead link to an empty screen
 * on two of the most-visited pages is worse than no link.
 *
 * The query shares Weight's own key, so arriving from either screen finds the
 * report already in cache.
 */
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { weight } from "../lib/api";
import { DASH } from "../lib/format";

/** Matches the Weight screen's window, so the two share one cache entry. */
const DAYS = 180;

/** Beyond this the newest reading describes then, not now. */
const STALE_DAYS = 21;

export function WeightGlance({ from }: { from: "today" | "food" }) {
  const { data } = useQuery({
    queryKey: ["weight", DAYS],
    queryFn: () => weight(DAYS),
    staleTime: 60_000,
  });

  // No data, or none in the window: say nothing. An account that has never
  // weighed in shouldn't be nagged about it from two other screens.
  if (!data || data.count === 0) return null;

  const trend = data.trendKg != null ? `${data.trendKg.toFixed(1)} kg` : DASH;
  const stale = data.daysSinceLatest != null && data.daysSinceLatest > STALE_DAYS;

  return (
    <p
      style={{
        fontSize: "var(--fs-base)",
        lineHeight: 1.6,
        color: "var(--mut)",
        // On Food this sits under the averages paragraph and is a change of
        // subject — the scale, not the log — so it needs more air above it
        // than the 8px that paragraph leaves behind.
        margin: from === "today" ? "14px 0 0" : "24px 0 4px",
        maxWidth: "58ch",
      }}
    >
      {sentence({ from, trend, stale, days: data.daysSinceLatest, rate: data.rateKgPerWeek })}{" "}
      <Link className="underlined" to="/weight" style={{ whiteSpace: "nowrap" }}>
        {from === "food" ? "See it against the scale" : "Weight"}
      </Link>
    </p>
  );
}

function sentence({
  from,
  trend,
  stale,
  days,
  rate,
}: {
  from: "today" | "food";
  trend: string;
  stale: boolean;
  days: number | null;
  rate: number | null;
}): string {
  if (stale) {
    return `Your weight trend last read ${trend}, ${days} days ago.`;
  }
  if (from === "food") {
    // Food's whole screen is the calorie balance, so the useful thing to say
    // here is that the scale is the check on it.
    return `Your weight trend is ${trend}${
      rate != null
        ? `, moving ${rate < 0 ? "down" : "up"} ${Math.abs(rate).toFixed(2)} kg a week`
        : ""
    }.`;
  }
  return rate != null
    ? `Weight trend ${trend}, ${rate < 0 ? "down" : "up"} ${Math.abs(rate).toFixed(2)} kg a week.`
    : `Weight trend ${trend}.`;
}
