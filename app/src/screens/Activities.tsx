import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { cachedActivities, type CachedActivity } from "../lib/api";
import { Empty, ErrorNote, Loading, PageHeader } from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import {
  DASH,
  duration,
  isRun,
  km,
  monthLabel,
  pace,
  parseLocal,
  shortDate,
  speed,
  sportLabel,
} from "../lib/format";

const PAGE = 40;

export function Activities() {
  const [limit, setLimit] = useState(PAGE);
  const { data, isLoading, error, isFetching } = useQuery({
    queryKey: ["activities", limit],
    queryFn: () => cachedActivities(limit),
    placeholderData: (prev) => prev,
  });

  if (isLoading) return <Loading />;
  if (error) return <ErrorNote error={error} />;

  const activities = data ?? [];
  if (!activities.length) {
    return (
      <>
        <PageHeader
          eyebrow="Nothing cached"
          title="Activities"
          lede="Every session on this machine, newest first."
          action={<RefreshButton />}
        />
        <Empty
          title="No activities cached."
          body="Sync from Settings to pull your history down. Nothing on this screen ever hits the network."
        />
      </>
    );
  }

  const total = activities.reduce((t, a) => t + (a.distanceM ?? 0), 0);
  const groups = groupByMonth(activities);
  const more = activities.length >= limit;

  return (
    <div className="screen">
      <PageHeader
        eyebrow={`${activities.length.toLocaleString()} shown · ${km(total, 0)}`}
        title="Activities"
        lede="Every session on this machine, newest first."
        action={<RefreshButton />}
      />

      {groups.map(([month, rows]) => (
        <div key={month}>
          <div className="eyebrow" style={{ margin: "26px 0 4px" }}>
            {month}
          </div>
          {rows.map((a) => (
            <ActivityRow key={a.activityId} a={a} />
          ))}
        </div>
      ))}

      {more && (
        <button
          className="quiet"
          style={{ marginTop: 26, fontSize: "var(--fs-small)", color: "var(--mut)" }}
          onClick={() => setLimit((l) => l + PAGE)}
          disabled={isFetching}
        >
          {isFetching ? "Loading…" : "Load earlier activities"}
        </button>
      )}
    </div>
  );
}

function ActivityRow({ a }: { a: CachedActivity }) {
  const d = parseLocal(a.startTimeLocal ?? a.localDate);
  // Runs read in min/km; anything with wheels reads in km/h. Sessions without
  // distance — strength, rope — get neither rather than a misleading 0:00.
  const rate = !a.distanceM
    ? DASH
    : isRun(a.typeKey) || a.typeKey?.includes("walk") || a.typeKey?.includes("hik")
      ? `${pace(a.distanceM, a.durationS)} /km`
      : speed(a.distanceM, a.durationS);

  return (
    <Link
      to="/activities/$activityId"
      params={{ activityId: String(a.activityId) }}
      className="row"
      style={{ color: "inherit" }}
    >
      <span
        style={{
          width: 50,
          flex: "none",
          color: "var(--faint)",
          fontSize: "var(--fs-caption)",
          letterSpacing: "0.03em",
        }}
      >
        {d ? shortDate(d) : DASH}
      </span>
      <span
        style={{
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {a.name ?? "Untitled"}
        <span style={{ color: "var(--faint)", fontSize: "var(--fs-small)", marginLeft: 10 }}>
          {sportLabel(a.typeKey)}
        </span>
      </span>
      <span className="mono" style={{ width: 82, textAlign: "right", fontSize: "var(--fs-base)" }}>
        {a.distanceM ? km(a.distanceM, 1) : DASH}
      </span>
      <span style={{ width: 70, textAlign: "right", color: "var(--mut)", fontSize: "var(--fs-small)" }}>
        {duration(a.durationS)}
      </span>
      <span style={{ width: 86, textAlign: "right", color: "var(--mut)", fontSize: "var(--fs-small)" }}>
        {rate}
      </span>
      <span style={{ width: 60, textAlign: "right", color: "var(--faint)", fontSize: "var(--fs-small)" }}>
        {a.avgHr ? Math.round(a.avgHr) : DASH}
      </span>
    </Link>
  );
}

function groupByMonth(activities: CachedActivity[]): Array<[string, CachedActivity[]]> {
  const groups: Array<[string, CachedActivity[]]> = [];
  for (const a of activities) {
    const d = parseLocal(a.startTimeLocal ?? a.localDate);
    const key = d ? monthLabel(d) : "Undated";
    const last = groups[groups.length - 1];
    if (last && last[0] === key) last[1].push(a);
    else groups.push([key, [a]]);
  }
  return groups;
}
