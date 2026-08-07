import { useNavigate, useParams } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import {
  activityDetails,
  activitySplits,
  cachedActivity,
  type CachedActivity,
} from "../lib/api";
import {
  BackLink,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  MetricRow,
  Rule,
  Unit,
} from "../components/ui";
import { hasData, type Point } from "../lib/chart";
import { zonePercentages, zoneTotal } from "../lib/derive";
import {
  DASH,
  duration,
  isRun,
  km,
  longDate,
  num,
  pace,
  parseLocal,
  speed,
  sportLabel,
  timeOfDay,
} from "../lib/format";

const ZONE_NAMES = ["Z1 recovery", "Z2 easy", "Z3 tempo", "Z4 threshold", "Z5 max"];

export function ActivityDetail() {
  const { activityId } = useParams({ from: "/activities/$activityId" });
  const navigate = useNavigate();
  const id = Number(activityId);

  const activity = useQuery({
    queryKey: ["activity", id],
    queryFn: () => cachedActivity(id),
  });

  if (activity.isLoading) return <Loading />;
  if (activity.error) return <ErrorNote error={activity.error} />;

  const a = activity.data;
  if (!a) {
    return (
      <Empty
        title="Not in the cache."
        body="This activity isn't in the local copy. A sync may bring it in."
      />
    );
  }

  const start = parseLocal(a.startTimeLocal ?? a.localDate);
  const paced = isRun(a.typeKey) || !!a.typeKey?.match(/walk|hik/);

  return (
    <div>
      <BackLink
        onClick={() => navigate({ to: "/activities" })}
        style={{ marginBottom: 26, color: "var(--mut)" }}
      >
        Activities
      </BackLink>

      <div className="eyebrow-lg">
        {sportLabel(a.typeKey)}
        {start && ` · ${longDate(start)} · ${timeOfDay(start)}`}
      </div>
      <h1 className="h1" style={{ margin: "16px 0 30px" }}>
        {a.name ?? "Untitled"}
      </h1>

      <MetricRow gap={46} style={{ marginBottom: 40 }}>
        {a.distanceM != null && a.distanceM > 0 && (
          <Metric size={31} label="Distance" value={km(a.distanceM, 2)} />
        )}
        <Metric size={31} label="Duration" value={duration(a.durationS)} />
        {a.movingDurationS != null && a.movingDurationS > 0 && (
          <Metric size={31} label="Moving" value={duration(a.movingDurationS)} />
        )}
        {a.distanceM != null && a.distanceM > 0 && (
          <Metric
            size={31}
            label={paced ? "Avg pace" : "Avg speed"}
            value={
              paced ? (
                <>
                  {pace(a.distanceM, a.durationS)}
                  <Unit size={19}> /km</Unit>
                </>
              ) : (
                speed(a.distanceM, a.durationS)
              )
            }
          />
        )}
        {a.avgHr != null && <Metric size={31} label="Avg HR" value={num(a.avgHr)} />}
        {a.maxHr != null && <Metric size={31} label="Max HR" value={num(a.maxHr)} />}
        {a.avgCadence != null && (
          <Metric size={31} label="Cadence" value={num(a.avgCadence)} />
        )}
        {a.elevationGain != null && a.elevationGain > 0 && (
          <Metric
            size={31}
            label="Ascent"
            value={
              <>
                {num(a.elevationGain)}
                <Unit size={19}> m</Unit>
              </>
            }
          />
        )}
      </MetricRow>

      <Zones activity={a} />
      <Charts id={id} />
      <Splits id={id} paced={paced} />
    </div>
  );
}

/* ----------------------------------------------------------------- zones --- */

/**
 * The zone breakdown, which is the number this whole app exists to show.
 * Rendered as stacked bars rather than a chart because the comparison that
 * matters is between the five rows, not across time.
 */
function Zones({ activity }: { activity: CachedActivity }) {
  const total = zoneTotal(activity);
  if (total <= 0) {
    return (
      <>
        <div className="eyebrow" style={{ marginBottom: 12 }}>
          Heart-rate zones
        </div>
        <p style={{ fontSize: 14, color: "var(--mut)", margin: "0 0 8px", maxWidth: "56ch" }}>
          No heart-rate data was recorded for this session, so there is no zone
          breakdown — not a session spent entirely in Z1.
        </p>
        <Rule m="30px 0 22px" />
      </>
    );
  }

  const pct = zonePercentages(activity);
  const hard = pct[2] + pct[3] + pct[4];

  return (
    <>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "baseline",
          marginBottom: 14,
        }}
      >
        <div className="eyebrow">Heart-rate zones</div>
        <div style={{ fontSize: 12.5, color: "var(--mut)" }}>
          {hard.toFixed(0)}% above Z2 · {duration(total)} tracked
        </div>
      </div>

      {ZONE_NAMES.map((name, i) => (
        <div key={name} style={{ padding: "10px 0 12px", borderBottom: "1px solid var(--line2)" }}>
          <div style={{ display: "flex", alignItems: "baseline", gap: 16 }}>
            <span style={{ flex: 1, fontSize: 14.5 }}>{name}</span>
            <span style={{ fontSize: 13, color: "var(--mut)" }}>
              {duration(activity.zoneSecs[i])}
            </span>
            <span className="mono" style={{ fontSize: 15, width: 56, textAlign: "right" }}>
              {pct[i].toFixed(0)}%
            </span>
          </div>
          <div className="bar" style={{ marginTop: 10 }}>
            <span
              style={{
                width: `${pct[i]}%`,
                // Z1/Z2 are the target; above that reads as accent so drift is
                // visible without reading the numbers.
                background: i < 2 ? "var(--mut)" : "var(--acc)",
              }}
            />
          </div>
        </div>
      ))}
      <Rule m="36px 0 22px" />
    </>
  );
}

/* ---------------------------------------------------------------- charts --- */

interface Metric3 {
  key: string;
  label: string;
  values: Point[];
  stroke: string;
  /** Faster is a smaller number, so the pace chart draws upside down. */
  invert?: boolean;
  format: (v: number) => string;
}

function Charts({ id }: { id: number }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["activityDetails", id],
    queryFn: () => activityDetails(id, 400),
    retry: false,
    staleTime: 5 * 60_000,
  });

  if (isLoading) return <Loading label="Fetching the time series from Garmin" />;
  // Charts are a live fetch, so an offline app should lose the charts, not the
  // page. The rest of this screen came from the cache and is still correct.
  if (error) {
    return (
      <p style={{ fontSize: 13, color: "var(--faint)", margin: "0 0 36px" }}>
        Couldn't fetch the time series — the cached summary above is unaffected.
      </p>
    );
  }

  const series = extractSeries(data);
  if (!series.length) {
    return (
      <p style={{ fontSize: 13, color: "var(--faint)", margin: "0 0 36px" }}>
        Garmin has no sampled series for this activity.
      </p>
    );
  }

  return (
    <>
      {series.map((s) => (
        <div key={s.key} style={{ marginBottom: 38 }}>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              alignItems: "baseline",
              marginBottom: 10,
            }}
          >
            <div className="eyebrow">{s.label}</div>
            <div style={{ fontSize: 12.5, color: "var(--mut)" }}>
              {summarise(s)}
            </div>
          </div>
          <LineChart
            series={[{ values: s.values, stroke: s.stroke, invert: s.invert }]}
            height={92}
          />
        </div>
      ))}
    </>
  );
}

function summarise(s: Metric3): string {
  const vals = s.values.filter((v): v is number => v != null);
  if (!vals.length) return "";
  const avg = vals.reduce((a, b) => a + b, 0) / vals.length;
  // "Best" for pace is the smallest number, not the largest.
  const best = s.invert ? Math.min(...vals) : Math.max(...vals);
  return `${s.format(avg)} avg · ${s.format(best)} ${s.invert ? "best" : "max"}`;
}

/** Decimal minutes to "5:01". */
function paceLabel(minPerKm: number): string {
  const m = Math.floor(minPerKm);
  const sec = Math.round((minPerKm - m) * 60);
  return sec === 60 ? `${m + 1}:00` : `${m}:${String(sec).padStart(2, "0")}`;
}

/**
 * Garmin's `/details` payload is a column-oriented table: `metricDescriptors`
 * names each column, `activityDetailMetrics[].metrics` holds the rows. Pull
 * out only the three columns the design charts.
 */
function extractSeries(payload: unknown): Metric3[] {
  const p = payload as {
    metricDescriptors?: Array<{ key?: string; metricsIndex?: number }>;
    activityDetailMetrics?: Array<{ metrics?: Array<number | null> }>;
  } | null;
  if (!p?.metricDescriptors || !p.activityDetailMetrics) return [];

  const bpm = (v: number) => `${v.toFixed(0)} bpm`;
  const wanted: Array<{
    keys: string[];
    label: string;
    stroke: string;
    invert?: boolean;
    format: (v: number) => string;
    scale?: (v: number) => number;
  }> = [
    { keys: ["directHeartRate"], label: "Heart rate", stroke: "var(--acc)", format: bpm },
    {
      // Garmin reports speed in m/s. Converted to minutes per kilometre it is
      // the unit every run here is read in, and the chart is inverted so faster
      // still draws higher. A near-zero speed is a pause, not a 300 min/km lap.
      keys: ["directSpeed"],
      label: "Pace",
      stroke: "var(--fg)",
      invert: true,
      scale: (v) => (v > 0.4 ? 1000 / v / 60 : NaN),
      format: (v) => `${paceLabel(v)} /km`,
    },
    {
      keys: ["directRunCadence", "directDoubleCadence"],
      label: "Cadence",
      stroke: "var(--mut)",
      format: (v) => `${v.toFixed(0)} spm`,
    },
    {
      keys: ["directElevation"],
      label: "Elevation",
      stroke: "var(--mut)",
      format: (v) => `${v.toFixed(0)} m`,
    },
  ];

  const out: Metric3[] = [];
  for (const w of wanted) {
    const desc = p.metricDescriptors.find(
      (d) => d.key && w.keys.includes(d.key) && d.metricsIndex != null,
    );
    if (!desc) continue;
    const idx = desc.metricsIndex!;
    const values = p.activityDetailMetrics.map((row) => {
      const v = row.metrics?.[idx];
      if (v == null || !isFinite(v)) return null;
      const scaled = w.scale ? w.scale(v) : v;
      return isFinite(scaled) ? scaled : null;
    });
    if (hasData(values)) {
      out.push({
        key: desc.key!,
        label: w.label,
        values,
        stroke: w.stroke,
        invert: w.invert,
        format: w.format,
      });
    }
  }
  return out;
}

/* ---------------------------------------------------------------- splits --- */

interface Lap {
  lapIndex?: number;
  distance?: number;
  duration?: number;
  averageHR?: number;
  averageRunCadence?: number;
  elevationGain?: number;
}

function Splits({ id, paced }: { id: number; paced: boolean }) {
  const { data, isLoading, error } = useQuery({
    queryKey: ["splits", id],
    queryFn: () => activitySplits(id),
    retry: false,
    staleTime: 5 * 60_000,
  });

  if (isLoading || error) return null;

  const laps = ((data as { lapDTOs?: Lap[] } | null)?.lapDTOs ?? []).filter(
    (l) => (l.duration ?? 0) > 0,
  );
  if (laps.length < 2) return null;

  // Bars are scaled against the slowest lap so the fastest fills the row —
  // an absolute scale would leave every bar near-identical.
  const rates = laps.map((l) =>
    l.distance && l.duration ? l.duration / (l.distance / 1000) : null,
  );
  const valid = rates.filter((r): r is number => r != null);
  const slowest = valid.length ? Math.max(...valid) : 1;
  const fastest = valid.length ? Math.min(...valid) : 1;

  return (
    <>
      <Rule m="8px 0 24px" />
      <div className="eyebrow" style={{ marginBottom: 8 }}>
        Splits
      </div>
      {laps.map((l, i) => {
        const rate = rates[i];
        const width =
          rate != null && slowest > fastest
            ? 20 + ((slowest - rate) / (slowest - fastest)) * 80
            : 60;
        return (
          <div
            key={i}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 16,
              padding: "8px 0",
              borderBottom: "1px solid var(--line2)",
              fontSize: 13.5,
            }}
          >
            <span style={{ width: 26, color: "var(--faint)" }}>{l.lapIndex ?? i + 1}</span>
            <span className="mono" style={{ width: 70, fontSize: 13.5 }}>
              {l.distance
                ? paced
                  ? pace(l.distance, l.duration)
                  : speed(l.distance, l.duration)
                : duration(l.duration)}
            </span>
            <span className="bar" style={{ flex: 1 }}>
              <span style={{ width: `${width}%` }} />
            </span>
            <span style={{ width: 62, textAlign: "right", color: "var(--mut)" }}>
              {l.averageHR ? `${Math.round(l.averageHR)} bpm` : DASH}
            </span>
            <span
              style={{ width: 56, textAlign: "right", color: "var(--faint)", fontSize: 12.5 }}
            >
              {l.distance ? km(l.distance, 2) : DASH}
            </span>
          </div>
        );
      })}
    </>
  );
}
