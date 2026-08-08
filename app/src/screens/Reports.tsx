import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { cachedActivitiesSince, cachedDaily } from "../lib/api";
import {
  byWeek,
  dailySeries,
  easyHardSplit,
  fuel,
  hydration,
  pick,
  type Fuel,
  type Hydration,
  type Week,
} from "../lib/derive";
import { mean } from "../lib/chart";
import {
  AxisLabels,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  PageHeader,
  Unit,
} from "../components/ui";
import { RefreshButton } from "../components/Refresh";
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
      <div>
        <PageHeader
          eyebrow="No weeks yet"
          title="The Weekly"
          lede="Monday to Sunday, written up and set against the week before."
          action={<RefreshButton />}
        />
        <Empty
          title="No weeks to report on."
          body="The weekly report is built from cached activities. Sync some history and it writes itself."
        />
      </div>
    );
  }

  const week = weeks[Math.min(offset, weeks.length - 1)];
  const prior = weeks[Math.min(offset + 1, weeks.length - 1)];
  const rows = dailySeries(daily.data ?? [], 120);

  const inWeek = rows.filter((r) => r.date >= week.start && r.date <= week.end);
  const avgSleep = mean(pick(inWeek, "sleepSecs"));
  const avgRhr = mean(pick(inWeek, "restingHr"));
  const split = easyHardSplit(week.activities);
  // The whole week, not a trailing window — `inWeek` is already the seven days
  // being reported on, so the fuel figures line up with every other number
  // on the page.
  const food = fuel(inWeek, inWeek.length || 1);
  const water = hydration(inWeek, inWeek.length || 1);

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
    <div className="screen">
      {/* Which issue you're looking at goes in the header's eyebrow. This used
          to sit the issue number beside the title over a full-width `--fg`
          rule — the only hairline in the app drawn at text weight, which read
          as a masthead on a screen that isn't one. */}
      <PageHeader
        eyebrow={`No. ${weeks.length - offset} · ${range(week)}`}
        title="The Weekly"
        lede="Monday to Sunday, written up and set against the week before."
        action={<RefreshButton />}
      />

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1.35fr 1fr",
          gap: 44,
        }}
      >
        <div>
          <div className="serif" style={{ fontSize: 26, lineHeight: 1.3, marginBottom: 14 }}>
            {headline(week, prior, split)}
          </div>
          <p style={{ fontSize: "var(--fs-md)", lineHeight: 1.72, margin: "0 0 14px", textWrap: "pretty" }}>
            {body(week, prior)}
          </p>
          <p style={{ fontSize: "var(--fs-md)", lineHeight: 1.72, color: "var(--mut)", margin: 0, textWrap: "pretty" }}>
            {recoveryBody(avgSleep, avgRhr, split, food, water)}
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

      <div className="eyebrow" style={{ margin: "58px 0 14px" }}>
        Daily distance
      </div>
      <LineChart
        series={[{ values: perDay, width: 1.4, fill: true, format: (v) => `${v.toFixed(1)} km` }]}
        height={80}
        pad={6}
        labels={DAY_LABELS}
      />
      <AxisLabels labels={DAY_LABELS} />

      <div className="eyebrow" style={{ margin: "60px 0 14px" }}>
        Sessions
      </div>
      {week.activities.map((a) => (
        <div key={a.activityId} className="row-static">
          <span style={{ flex: 1 }}>
            {a.name ?? "Untitled"}
            <span style={{ color: "var(--faint)", fontSize: "var(--fs-small)", marginLeft: 10 }}>
              {(a.typeKey ?? "").replace(/_/g, " ")}
            </span>
          </span>
          <span className="mono" style={{ width: 88, textAlign: "right", fontSize: "var(--fs-base)" }}>
            {a.distanceM ? km(a.distanceM) : DASH}
          </span>
          <span style={{ width: 76, textAlign: "right", color: "var(--mut)", fontSize: "var(--fs-small)" }}>
            {duration(a.durationS)}
          </span>
        </div>
      ))}

      <div style={{ display: "flex", gap: 24, marginTop: 40, fontSize: "var(--fs-small)" }}>
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
  food: Fuel,
  water: Hydration | null,
): string {
  const parts: string[] = [];
  if (avgSleep != null) parts.push(`Sleep averaged ${hoursMinutes(avgSleep)}.`);
  if (avgRhr != null) parts.push(`Resting heart rate averaged ${num(avgRhr)} bpm.`);

  // Two logged days can't describe a week, so below that the sentence reports
  // the coverage instead of an average computed off it.
  if (food.logged >= 3 && food.avgBalance != null) {
    const avg = Math.round(food.avgBalance);
    parts.push(
      `Food was logged on ${food.logged} of ${food.window} days, averaging ${avg > 0 ? "a surplus of" : "a deficit of"} ${num(Math.abs(avg))} kcal against ${num(Math.round(food.avgBurn ?? 0))} burned.`,
    );
  } else if (food.logged > 0) {
    parts.push(
      `Food was logged on ${food.logged} of ${food.window} ${food.window === 1 ? "day" : "days"} — too thin to average.`,
    );
  } else if (food.avgBurn != null) {
    parts.push(`No food logged this week, against ${num(Math.round(food.avgBurn))} kcal a day burned.`);
  }

  // Silent for accounts that don't track it, rather than reporting a week of
  // zeroes as a week of drinking nothing.
  if (water) {
    const litres = `${(water.avgMl / 1000).toFixed(2)} L`;
    parts.push(
      water.goalMl != null
        ? `Water averaged ${litres} a day across ${water.logged} logged ${water.logged === 1 ? "day" : "days"}, against a ${(water.goalMl / 1000).toFixed(2)} L goal.`
        : `Water averaged ${litres} a day across ${water.logged} logged ${water.logged === 1 ? "day" : "days"}.`,
    );
  }
  if (split) {
    parts.push(
      split.hardPct > 30
        ? `${split.hardPct.toFixed(0)}% of tracked heart-rate time was above Z2, across ${split.counted} ${split.counted === 1 ? "session" : "sessions"} that recorded it — the 80/20 model wants that nearer 20%.`
        : `${split.easyPct.toFixed(0)}% of tracked heart-rate time was in Z1–Z2, which is where a base-building week should sit.`,
    );
  }
  return parts.join(" ") || "No recovery metrics recorded for this week.";
}
