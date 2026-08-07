import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { cachedActivitiesSince, cachedDaily } from "../lib/api";
import { byWeek, dailySeries, easyHardSplit, pick, type Week } from "../lib/derive";
import { mean } from "../lib/chart";
import {
  AxisLabels,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  Rule,
  Unit,
} from "../components/ui";
import { DASH, daysAgo, duration, hoursMinutes, km, num, parseLocal } from "../lib/format";

const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function Reports() {
  const [offset, setOffset] = useState(0); // 0 = most recent week with activity

  const acts = useQuery({
    queryKey: ["activitiesSince", 120],
    queryFn: () => cachedActivitiesSince(daysAgo(120)),
  });
  const daily = useQuery({ queryKey: ["daily", 120], queryFn: () => cachedDaily(120) });

  if (acts.isLoading || daily.isLoading) return <Loading />;
  if (acts.error) return <ErrorNote error={acts.error} />;

  const weeks = byWeek(acts.data ?? []);
  if (!weeks.length) {
    return (
      <Empty
        title="No weeks to report on."
        body="The weekly report is built from cached activities. Sync some history and it writes itself."
      />
    );
  }

  const week = weeks[Math.min(offset, weeks.length - 1)];
  const prior = weeks[Math.min(offset + 1, weeks.length - 1)];
  const rows = dailySeries(daily.data ?? [], 120);

  const inWeek = rows.filter((r) => r.date >= week.start && r.date <= week.end);
  const avgSleep = mean(pick(inWeek, "sleepSecs"));
  const avgRhr = mean(pick(inWeek, "restingHr"));
  const split = easyHardSplit(week.activities);

  // One bar per day of that specific week, Monday first.
  const perDay = DAY_LABELS.map((_, i) => {
    const d = parseLocal(week.start)!;
    d.setDate(d.getDate() + i);
    const key = [
      d.getFullYear(),
      String(d.getMonth() + 1).padStart(2, "0"),
      String(d.getDate()).padStart(2, "0"),
    ].join("-");
    return (
      week.activities
        .filter((a) => a.localDate === key)
        .reduce((t, a) => t + (a.distanceM ?? 0), 0) / 1000
    );
  });

  return (
    <div>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          borderBottom: "1px solid var(--fg)",
          paddingBottom: 12,
        }}
      >
        <div className="serif" style={{ fontSize: 30 }}>
          The Weekly
        </div>
        <div
          style={{
            font: "400 10.5px/1 'Instrument Sans', sans-serif",
            letterSpacing: "0.13em",
            textTransform: "uppercase",
            color: "var(--mut)",
          }}
        >
          No. {weeks.length - offset} · {range(week)}
        </div>
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1.35fr 1fr",
          gap: 44,
          marginTop: 34,
        }}
      >
        <div>
          <div className="serif" style={{ fontSize: 26, lineHeight: 1.3, marginBottom: 14 }}>
            {headline(week, prior, split)}
          </div>
          <p style={{ fontSize: 15, lineHeight: 1.72, margin: "0 0 14px", textWrap: "pretty" }}>
            {body(week, prior)}
          </p>
          <p style={{ fontSize: 15, lineHeight: 1.72, color: "var(--mut)", margin: 0, textWrap: "pretty" }}>
            {recoveryBody(avgSleep, avgRhr, split)}
          </p>
        </div>

        <div style={{ borderLeft: "1px solid var(--line)", paddingLeft: 28 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 24 }}>
            <Metric
              size={26}
              label="Distance"
              value={
                week.distanceM > 0 ? (
                  <>
                    {(week.distanceM / 1000).toFixed(1)}
                    <Unit size={16}> km</Unit>
                  </>
                ) : (
                  DASH
                )
              }
            />
            <Metric size={26} label="Time" value={hoursMinutes(week.durationS)} />
            <Metric size={26} label="Sessions" value={num(week.activities.length)} />
            <Metric
              size={26}
              label="Avg sleep"
              value={avgSleep ? hoursMinutes(avgSleep) : DASH}
            />
            {split && (
              <Metric
                size={26}
                label="Time above Z2"
                value={`${split.hardPct.toFixed(0)}%`}
                accent={split.hardPct > 30}
              />
            )}
          </div>
        </div>
      </div>

      <Rule m="38px 0 20px" />
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Daily distance
      </div>
      <LineChart series={[{ values: perDay, width: 1.4 }]} height={80} pad={6} />
      <AxisLabels labels={DAY_LABELS} />

      <Rule m="40px 0 20px" />
      <div className="eyebrow" style={{ marginBottom: 14 }}>
        Sessions
      </div>
      {week.activities.map((a) => (
        <div key={a.activityId} className="row-static">
          <span style={{ flex: 1 }}>
            {a.name ?? "Untitled"}
            <span style={{ color: "var(--faint)", fontSize: 12.5, marginLeft: 10 }}>
              {(a.typeKey ?? "").replace(/_/g, " ")}
            </span>
          </span>
          <span className="mono" style={{ width: 80, textAlign: "right", fontSize: 14 }}>
            {a.distanceM ? km(a.distanceM) : DASH}
          </span>
          <span style={{ width: 70, textAlign: "right", color: "var(--mut)", fontSize: 13 }}>
            {duration(a.durationS)}
          </span>
        </div>
      ))}

      <div style={{ display: "flex", gap: 24, marginTop: 40, fontSize: 13 }}>
        <button
          className="quiet"
          style={{ color: "var(--mut)" }}
          onClick={() => setOffset((o) => Math.min(o + 1, weeks.length - 1))}
          disabled={offset >= weeks.length - 1}
        >
          Earlier week
        </button>
        <button
          className="quiet"
          style={{ color: "var(--mut)" }}
          onClick={() => setOffset((o) => Math.max(o - 1, 0))}
          disabled={offset === 0}
        >
          Later week
        </button>
        <span style={{ color: "var(--faint)" }}>
          Built from the local cache — no network needed.
        </span>
      </div>
    </div>
  );
}

const MONTH_ABBR = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

function range(w: Week): string {
  const a = parseLocal(w.start)!;
  const b = parseLocal(w.end)!;
  return `${a.getDate()} ${MONTH_ABBR[a.getMonth()]} – ${b.getDate()} ${MONTH_ABBR[b.getMonth()]}`;
}

function headline(
  week: Week,
  prior: Week,
  split: ReturnType<typeof easyHardSplit>,
): string {
  if (week === prior) return "Your first week in the cache";
  if (split && split.hardPct <= 25 && week.runs >= 2)
    return "The closest you've come to an 80/20 week";
  if (week.distanceM > prior.distanceM * 1.25) return "Your biggest week of the block";
  if (week.activities.length === 0) return "A week off";
  if (split && split.hardPct > 60) return "A hard week, front to back";
  return "A steady week";
}

function body(week: Week, prior: Week): string {
  const parts = [
    `${week.activities.length} ${week.activities.length === 1 ? "session" : "sessions"}`,
  ];
  if (week.distanceM > 0) parts.push(km(week.distanceM));
  parts.push(hoursMinutes(week.durationS) + " of recorded time");

  let sentence = parts.join(", ") + ".";
  if (week !== prior && prior.durationS > 0) {
    const change = ((week.durationS - prior.durationS) / prior.durationS) * 100;
    sentence += ` That's ${Math.abs(change).toFixed(0)}% ${change >= 0 ? "more" : "less"} time than the week before.`;
  }
  const longest = [...week.activities].sort(
    (a, b) => (b.durationS ?? 0) - (a.durationS ?? 0),
  )[0];
  if (longest) {
    sentence += ` The longest was ${longest.name ?? "an untitled session"} at ${duration(longest.durationS)}.`;
  }
  return sentence;
}

function recoveryBody(
  avgSleep: number | null,
  avgRhr: number | null,
  split: ReturnType<typeof easyHardSplit>,
): string {
  const parts: string[] = [];
  if (avgSleep != null) parts.push(`Sleep averaged ${hoursMinutes(avgSleep)}.`);
  if (avgRhr != null) parts.push(`Resting heart rate averaged ${num(avgRhr)} bpm.`);
  if (split) {
    parts.push(
      split.hardPct > 30
        ? `${split.hardPct.toFixed(0)}% of tracked heart-rate time was above Z2, across ${split.counted} ${split.counted === 1 ? "session" : "sessions"} that recorded it — the 80/20 model wants that nearer 20%.`
        : `${split.easyPct.toFixed(0)}% of tracked heart-rate time was in Z1–Z2, which is where a base-building week should sit.`,
    );
  }
  return parts.join(" ") || "No recovery metrics recorded for this week.";
}
