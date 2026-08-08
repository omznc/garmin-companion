import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { cachedDaily, type DailyMetrics } from "../lib/api";
import { dailySeries, hydrationMl, pick, sweatLossMl } from "../lib/derive";
import { hasData, mean, smooth, type Point } from "../lib/chart";
import {
  AxisLabels,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  PageHeader,
} from "../components/ui";
import { RefreshButton } from "../components/Refresh";
import { DASH, hoursMinutes, num, parseLocal, shortDate } from "../lib/format";

const RANGES = [
  { days: 1, label: "24 hours" },
  { days: 7, label: "7 days" },
  { days: 30, label: "30 days" },
  { days: 90, label: "90 days" },
  { days: 182, label: "6 months" },
  { days: 365, label: "Year" },
] as const;

/**
 * The rolling-mean window for the dashed trend line. Fixed at 7 it swallowed
 * a month-long window whole and barely moved across a year, so it scales with
 * the range instead.
 */
const trendWindow = (days: number) =>
  days >= 180 ? 21 : days >= 90 ? 7 : days >= 30 ? 5 : 3;

interface Track {
  key: keyof DailyMetrics;
  label: string;
  format: (v: number) => string;
  stroke: string;
  /** How to read a rise: for resting HR and stress, up is worse. */
  upIsGood: boolean;
  /**
   * How to read the value off a row, where the raw field isn't it. Hydration
   * is the case: Garmin writes 0 rather than null on days nothing tracked it,
   * and a track that reads those literally draws a flat line along the floor
   * and reports a confident downward "trend" made entirely of blanks.
   */
  value?: (d: DailyMetrics) => number | null;
}

/** A track's series, honouring its accessor. */
const read = (t: Track, rows: DailyMetrics[]): Point[] =>
  t.value ? rows.map(t.value) : pick(rows, t.key);

const TRACKS: Track[] = [
  { key: "sleepSecs", label: "Sleep", format: hoursMinutes, stroke: "var(--acc)", upIsGood: true },
  { key: "bodyBatteryHigh", label: "Body battery", format: (v) => num(v), stroke: "var(--fg)", upIsGood: true },
  { key: "restingHr", label: "Resting HR", format: (v) => `${num(v)} bpm`, stroke: "var(--acc)", upIsGood: false },
  { key: "hrvLastNight", label: "HRV", format: (v) => `${num(v)} ms`, stroke: "var(--fg)", upIsGood: true },
  { key: "trainingReadiness", label: "Readiness", format: (v) => num(v), stroke: "var(--mut)", upIsGood: true },
  { key: "stressAvg", label: "Stress", format: (v) => num(v), stroke: "var(--mut)", upIsGood: false },
  { key: "steps", label: "Steps", format: (v) => num(v), stroke: "var(--mut)", upIsGood: true },
  // Only renders for accounts that actually log it — `hasData` sees an all-zero
  // column as empty once the accessor has stripped the zeros, so the track
  // drops off the screen for everyone else rather than showing a flat nothing.
  {
    key: "hydrationMl",
    label: "Hydration",
    format: (v) => `${(v / 1000).toFixed(2)} L`,
    stroke: "var(--fg)",
    upIsGood: true,
    value: hydrationMl,
  },
  // Sweat loss is computed off the session rather than logged by hand, so it
  // survives on accounts where intake never does. No `upIsGood` reading is
  // honest here — sweating more means you trained harder, not better or worse
  // — so it's marked true and the trend sentence stays neutral by saying so.
  {
    key: "sweatLossMl",
    label: "Sweat loss",
    format: (v) => `${(v / 1000).toFixed(2)} L`,
    stroke: "var(--mut)",
    upIsGood: true,
    value: sweatLossMl,
  },
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
  const populated = TRACKS.filter((t) => hasData(read(t, rows)));

  const label = RANGES.find((r) => r.days === days)!.label;

  if (!populated.length) {
    return (
      <>
        <PageHeader
          eyebrow={label}
          title="Health"
          lede="Resting HR, HRV, sleep and readiness, as far back as the cache goes."
          action={<RefreshButton />}
        />
        <RangePicker days={days} onPick={setDays} />
        {/* A short window with nothing in it is a different problem from an
            empty cache — today's row simply may not have synced yet. */}
        {days <= 7 ? (
          <Empty
            title="Nothing recorded in this window."
            body={`No wellness readings in the last ${label.toLowerCase()}. Widen the range above, or run a sync from Settings.`}
          />
        ) : (
          <Empty
            title="No wellness data cached."
            body="Resting HR, HRV, sleep and readiness come from the daily sync. Run one from Settings and this fills in."
          />
        )}
      </>
    );
  }

  const coverage = (data ?? []).length;

  return (
    <div className="screen">
      <PageHeader
        eyebrow={`${coverage} ${coverage === 1 ? "day" : "days"} cached`}
        title="Health"
        lede="Every figure is the average across the window you pick. Each chart is scaled to its own range — the shape is the point, not the absolute height."
        action={<RefreshButton />}
        space={20}
      />

      <RangePicker days={days} onPick={setDays} />

      {populated.map((t) => (
        <TrackChart key={String(t.key)} track={t} rows={rows} days={days} />
      ))}
    </div>
  );
}

function RangePicker({
  days,
  onPick,
}: {
  days: number;
  onPick: (days: number) => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexWrap: "wrap",
        gap: 18,
        rowGap: 10,
        fontSize: "var(--fs-small)",
        color: "var(--faint)",
        marginBottom: 48,
      }}
    >
      {RANGES.map((r) => (
        <button
          key={r.days}
          onClick={() => onPick(r.days)}
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
  const values = read(track, rows);
  // Real readings only, which for a track with an accessor is not the same as
  // the populated rows — and blanks must never average in as zero.
  const real = values.filter((v): v is number => v != null);
  const avg = mean(values);
  const latest = real.length ? real[real.length - 1] : null;
  // A rolling mean over four points is a straight line; below that the trend
  // line says nothing the raw series doesn't already say.
  const trend = real.length >= 5 ? smooth(values, trendWindow(days)) : null;

  return (
    <div style={{ marginBottom: 52 }}>
      <div style={{ display: "flex", alignItems: "flex-end", gap: 24, marginBottom: 14 }}>
        <div style={{ flex: "none" }}>
          <div className="mono" style={{ fontSize: 33, lineHeight: 1, letterSpacing: "-0.04em" }}>
            {avg != null ? track.format(avg) : DASH}
          </div>
          <div
            style={{
              font: "400 var(--fs-micro)/1 'Instrument Sans', sans-serif",
              letterSpacing: "0.11em",
              textTransform: "uppercase",
              color: "var(--mut)",
              marginTop: 10,
            }}
          >
            {track.label}
          </div>
          {/* The average is over the days that carry a reading, not over the
              window — say which, and keep the latest figure reachable. */}
          <div style={{ fontSize: "var(--fs-caption)", lineHeight: 1.4, color: "var(--faint)", marginTop: 7 }}>
            {real.length > 1
              ? `avg of ${real.length} days · latest ${track.format(latest!)}`
              : real.length === 1
                ? "single reading"
                : "no readings"}
          </div>
        </div>
        <div
          style={{
            flex: 1,
            fontSize: "var(--fs-base)",
            lineHeight: 1.6,
            color: "var(--mut)",
            paddingBottom: 4,
            textWrap: "pretty",
          }}
        >
          {sentence(track, values)}
        </div>
      </div>
      {/* One day is a number, not a shape — the chart would be a dot pinned to
          the left edge, so it's dropped rather than drawn. */}
      {rows.length > 1 && (
        <>
          <LineChart
            series={[
              {
                values,
                stroke: track.stroke,
                width: 1.25,
                fill: true,
                name: trend ? track.label : undefined,
                format: track.format,
              },
              // The smoothed line shares the raw series' scale so the two are
              // readable against each other.
              ...(trend
                ? [{
                    values: trend,
                    stroke: "var(--faint)",
                    width: 1,
                    dashed: true,
                    name: "Trend",
                    format: track.format,
                  }]
                : []),
            ]}
            height={76}
            shareScale
            labels={rows.map((r) => dayLabel(r.date))}
          />
          <AxisLabels labels={axisFor(rows, days)} />
        </>
      )}
    </div>
  );
}

/**
 * A plain-language read of the trend: the recent third against the earlier
 * two-thirds. Deliberately refuses to comment when the change is inside noise.
 */
function sentence(track: Track, values: Point[]): string {
  const real = values.filter((v): v is number => v != null);
  if (!real.length) return "Nothing recorded in this window.";
  // Short windows: state the reading rather than dressing two points up as a
  // direction. `slice(-0)` returns the whole array, so the cut is guarded too.
  if (real.length < 4 || values.length < 6) {
    return real.length === 1
      ? `One reading in this window: ${track.format(real[0])}. Widen the range for a trend.`
      : `${real.length} readings in this window, averaging ${track.format(mean(real)!)}. Widen the range for a trend.`;
  }

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

/** Heads the hover readout. Every day gets one, unlike the four axis markers,
 *  so pointing at a peak on a year-long chart still says which day it was. */
function dayLabel(date: string): string {
  const d = parseLocal(date);
  return d ? shortDate(d) : date;
}

/**
 * Four evenly spaced markers under the chart. Month names carry a quarter or a
 * year; on a week they'd read "Aug Aug Aug Aug", so short windows get dates.
 */
function axisFor(rows: DailyMetrics[], days: number): string[] {
  if (rows.length < 4) return [];
  return [0, 1, 2, 3].map((i) => {
    const d = parseLocal(rows[Math.floor((i / 3) * (rows.length - 1))].date);
    if (!d) return "";
    return days <= 31
      ? `${d.getDate()} ${MONTH_ABBR[d.getMonth()]}`
      : MONTH_ABBR[d.getMonth()];
  });
}
