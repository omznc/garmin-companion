import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { cachedDaily, type DailyMetrics } from "../lib/api";
import { dailySeries, latest, pick } from "../lib/derive";
import { hasData, mean, smooth, type Point } from "../lib/chart";
import {
  AxisLabels,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  PageTitle,
} from "../components/ui";
import { DASH, hoursMinutes, num, parseLocal } from "../lib/format";

const RANGES = [
  { days: 90, label: "90 days" },
  { days: 182, label: "6 months" },
  { days: 365, label: "Year" },
] as const;

interface Track {
  key: keyof DailyMetrics;
  label: string;
  format: (v: number) => string;
  stroke: string;
  /** How to read a rise: for resting HR and stress, up is worse. */
  upIsGood: boolean;
}

const TRACKS: Track[] = [
  { key: "sleepSecs", label: "Sleep", format: hoursMinutes, stroke: "var(--acc)", upIsGood: true },
  { key: "bodyBatteryHigh", label: "Body battery", format: (v) => num(v), stroke: "var(--fg)", upIsGood: true },
  { key: "restingHr", label: "Resting HR", format: (v) => `${num(v)} bpm`, stroke: "var(--acc)", upIsGood: false },
  { key: "hrvLastNight", label: "HRV", format: (v) => `${num(v)} ms`, stroke: "var(--fg)", upIsGood: true },
  { key: "trainingReadiness", label: "Readiness", format: (v) => num(v), stroke: "var(--mut)", upIsGood: true },
  { key: "stressAvg", label: "Stress", format: (v) => num(v), stroke: "var(--mut)", upIsGood: false },
  { key: "steps", label: "Steps", format: (v) => num(v), stroke: "var(--mut)", upIsGood: true },
];

export function Health() {
  const [days, setDays] = useState<number>(90);
  const { data, isLoading, error } = useQuery({
    queryKey: ["daily", days],
    queryFn: () => cachedDaily(days),
    placeholderData: (prev) => prev,
  });

  if (isLoading) return <Loading />;
  if (error) return <ErrorNote error={error} />;

  const rows = dailySeries(data ?? [], days);
  const populated = TRACKS.filter((t) => hasData(pick(rows, t.key)));

  if (!populated.length) {
    return (
      <>
        <PageTitle>Health</PageTitle>
        <Empty
          title="No wellness data cached."
          body="Resting HR, HRV, sleep and readiness come from the daily sync. Run one from Settings and this fills in."
        />
      </>
    );
  }

  const coverage = (data ?? []).length;

  return (
    <div>
      <PageTitle>Health</PageTitle>
      <p style={{ fontSize: 16, lineHeight: 1.7, color: "var(--mut)", margin: "0 0 12px", maxWidth: "60ch" }}>
        {coverage} {coverage === 1 ? "day" : "days"} of recorded data in the local
        cache. Each chart is scaled to its own range — the shape is the point,
        not the absolute height.
      </p>

      <div style={{ display: "flex", gap: 18, fontSize: 12.5, color: "var(--faint)", marginBottom: 48 }}>
        {RANGES.map((r) => (
          <button
            key={r.days}
            onClick={() => setDays(r.days)}
            style={{
              cursor: "pointer",
              color: days === r.days ? "var(--fg)" : "var(--faint)",
              borderBottom: `1px solid ${days === r.days ? "var(--acc)" : "transparent"}`,
              paddingBottom: 2,
            }}
          >
            {r.label}
          </button>
        ))}
      </div>

      {populated.map((t) => (
        <TrackChart key={String(t.key)} track={t} rows={rows} days={days} />
      ))}
    </div>
  );
}

function TrackChart({
  track,
  rows,
  days,
}: {
  track: Track;
  rows: DailyMetrics[];
  days: number;
}) {
  const values = pick(rows, track.key);
  const current = latest(rows, track.key);
  const trend = smooth(values, days > 120 ? 21 : 7);

  return (
    <div style={{ marginBottom: 52 }}>
      <div style={{ display: "flex", alignItems: "flex-end", gap: 24, marginBottom: 14 }}>
        <div style={{ flex: "none" }}>
          <div className="mono" style={{ fontSize: 33, lineHeight: 1, letterSpacing: "-0.04em" }}>
            {current ? track.format(current.value) : DASH}
          </div>
          <div
            style={{
              font: "400 10.5px/1 'Instrument Sans', sans-serif",
              letterSpacing: "0.11em",
              textTransform: "uppercase",
              color: "var(--mut)",
              marginTop: 10,
            }}
          >
            {track.label}
          </div>
        </div>
        <div
          style={{
            flex: 1,
            fontSize: 14,
            lineHeight: 1.6,
            color: "var(--mut)",
            paddingBottom: 4,
            textWrap: "pretty",
          }}
        >
          {sentence(track, values)}
        </div>
      </div>
      <LineChart
        series={[
          { values, stroke: track.stroke, width: 1.25 },
          // The smoothed line shares the raw series' scale so the two are
          // readable against each other.
          { values: trend, stroke: "var(--faint)", width: 1, dashed: true },
        ]}
        height={76}
        shareScale
      />
      <AxisLabels labels={axisFor(rows)} />
    </div>
  );
}

/**
 * A plain-language read of the trend: the recent third against the earlier
 * two-thirds. Deliberately refuses to comment when the change is inside noise.
 */
function sentence(track: Track, values: Point[]): string {
  const cut = Math.floor(values.length / 3);
  const recent = mean(values.slice(-cut));
  const earlier = mean(values.slice(0, -cut));
  if (recent == null || earlier == null) return "Not enough history to call a trend yet.";

  const change = ((recent - earlier) / Math.abs(earlier)) * 100;
  if (Math.abs(change) < 3) {
    return `Flat across the window — recent average ${track.format(recent)}, against ${track.format(earlier)} before.`;
  }

  const rising = change > 0;
  const good = rising === track.upIsGood;
  return `${rising ? "Up" : "Down"} ${Math.abs(change).toFixed(0)}% on the earlier part of this window (${track.format(recent)} against ${track.format(earlier)}) — ${good ? "the direction you want" : "worth keeping an eye on"}.`;
}

const MONTH_ABBR = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** Four evenly spaced month markers under the chart. */
function axisFor(rows: DailyMetrics[]): string[] {
  if (rows.length < 4) return [];
  return [0, 1, 2, 3].map((i) => {
    const d = parseLocal(rows[Math.floor((i / 3) * (rows.length - 1))].date);
    return d ? MONTH_ABBR[d.getMonth()] : "";
  });
}
