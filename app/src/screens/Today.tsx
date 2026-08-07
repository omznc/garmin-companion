import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import {
  cachedActivities,
  cachedDaily,
  garminProfile,
  nutrition,
  type CachedActivity,
  type DailyMetrics,
} from "../lib/api";
import {
  attention,
  dailyDistance,
  dailySeries,
  easyHardSplit,
  hasZoneData,
  latest,
  pick,
  zonePercentages,
} from "../lib/derive";
import { mean } from "../lib/chart";
import { ZoneBar } from "../components/ZoneBar";
import { firstName, greeting } from "../lib/greeting";
import {
  AxisLabels,
  Bullet,
  Empty,
  ErrorNote,
  LineChart,
  Loading,
  Metric,
  MetricRow,
  Rule,
  Spark,
  Unit,
} from "../components/ui";
import {
  DASH,
  bpm,
  duration,
  hoursMinutes,
  isRun,
  isoDate,
  km,
  longDate,
  num,
  pace,
  parseLocal,
  shortDate,
  speed,
  sportLabel,
} from "../lib/format";

const DAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function Today() {
  const daily = useQuery({ queryKey: ["daily", 45], queryFn: () => cachedDaily(45) });
  const acts = useQuery({
    queryKey: ["activities", 60],
    queryFn: () => cachedActivities(60),
  });
  // Food is often not logged at all, so this one is allowed to be missing
  // without holding up the screen.
  const food = useQuery({ queryKey: ["nutrition", 7], queryFn: () => nutrition(7) });
  // Shares the sidebar's cache entry, so this costs nothing. A greeting that
  // has to wait for a name would be worse than one that never uses it.
  const profile = useQuery({
    queryKey: ["profile"],
    queryFn: garminProfile,
    staleTime: Infinity,
    retry: false,
  });

  if (daily.isLoading || acts.isLoading) return <Loading />;
  if (daily.error) return <ErrorNote error={daily.error} />;
  if (acts.error) return <ErrorNote error={acts.error} />;

  const rows = dailySeries(daily.data ?? [], 45);
  const activities = acts.data ?? [];

  if (!activities.length && !(daily.data ?? []).length) {
    return (
      <Empty
        title="Nothing cached yet."
        body={
          <>
            Run a sync to pull your Garmin history onto this machine. Everything
            on these screens is read from that local copy — this is the only
            screen that will tell you it's missing.
          </>
        }
        action={
          <Link className="cta" to="/settings">
            Go to settings
          </Link>
        }
      />
    );
  }

  const sleep = latest(rows, "sleepSecs");
  const battery = latest(rows, "bodyBatteryHigh");
  const readiness = latest(rows, "trainingReadiness");
  const hrv = latest(rows, "hrvLastNight");
  const rhr = latest(rows, "restingHr");
  // The morning number is only meaningful against your own baseline, so the
  // 30-day mean travels with it.
  const rhrBase = mean(pick(rows, "restingHr"));

  const lastSession = activities[0] ?? null;
  // Runs with HR, newest first — the drift strip and nothing else uses these.
  const trackedRuns = activities.filter((a) => isRun(a.typeKey) && hasZoneData(a)).slice(0, 8);
  const fuel = food.data?.days.find((d) => d.logged) ?? null;

  const week = dailyDistance(activities, 7);
  const weekTotal = week.reduce((a, b) => a + b, 0);
  const priorWeek = dailyDistance(activities, 14).slice(0, 7).reduce((a, b) => a + b, 0);
  const sessions = countSessions(activities, 7);

  const notes = attention(rows, activities);
  const summary = narrative(rows, activities);

  const today = new Date();
  // The chart runs to today, so the axis has to start on today's weekday + 1.
  const axis = Array.from({ length: 7 }, (_, i) => {
    const d = new Date();
    d.setDate(d.getDate() - (6 - i));
    return DAY_LABELS[(d.getDay() + 6) % 7];
  });

  return (
    <div>
      <div className="eyebrow-lg">{longDate(today)}</div>
      <h1
        className="serif"
        style={{
          fontSize: 44,
          lineHeight: 1.08,
          letterSpacing: "-0.015em",
          margin: "20px 0 26px",
        }}
      >
        {greeting({ name: firstName(profile.data?.fullName ?? profile.data?.displayName) })}
      </h1>
      <p
        style={{
          fontSize: 16.5,
          lineHeight: 1.72,
          margin: 0,
          maxWidth: "64ch",
          textWrap: "pretty",
        }}
      >
        {summary}
      </p>

      <MetricRow style={{ margin: "46px 0 0" }}>
        <Metric
          label="Sleep"
          value={
            sleep ? (
              <>
                {Math.floor(sleep.value / 3600)}
                <Unit>h</Unit> {String(Math.round((sleep.value % 3600) / 60)).padStart(2, "0")}
                <Unit>m</Unit>
              </>
            ) : (
              DASH
            )
          }
        />
        <Metric label="Body battery" value={battery ? num(battery.value) : DASH} />
        <Metric label="Readiness" value={readiness ? num(readiness.value) : DASH} />
        <Metric
          label="HRV"
          value={
            hrv ? (
              <>
                {num(hrv.value)}
                <Unit size={22}> ms</Unit>
              </>
            ) : (
              DASH
            )
          }
        />
        <Metric
          label={
            rhr && rhrBase != null
              ? `Resting HR · ${rhr.value > rhrBase ? "+" : ""}${(rhr.value - rhrBase).toFixed(0)} vs 30d`
              : "Resting HR"
          }
          // Only a rise gets the accent. A resting heart rate below your own
          // baseline is good news and shouldn't be dressed as a warning.
          accent={rhr != null && rhrBase != null && rhr.value - rhrBase >= 3}
          value={rhr ? bpm(rhr.value) : DASH}
        />
      </MetricRow>

      {fuel && (
        <p style={{ fontSize: 14, color: "var(--mut)", margin: "26px 0 0", maxWidth: "58ch" }}>
          {fuelSentence(fuel.consumedKcal, fuel.totalBurnKcal, fuel.balanceKcal, fuel.date)}
        </p>
      )}

      <Rule m="52px 0 26px" />

      {notes.length > 0 && (
        <>
          <div className="eyebrow" style={{ marginBottom: 16 }}>
            Attention
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 13 }}>
            {notes.map((n, i) => (
              <Bullet key={i} accent={n.accent}>
                {n.text}
                {n.spark && <Spark values={n.spark} />}
                {n.link && (
                  <Link
                    className="underlined"
                    to={n.link.to}
                    style={{ marginLeft: 6, whiteSpace: "nowrap" }}
                  >
                    {n.link.label}
                  </Link>
                )}
              </Bullet>
            ))}
          </div>
          <Rule m="44px 0 26px" />
        </>
      )}

      {lastSession && <LastSession activity={lastSession} />}

      {trackedRuns.length >= 2 && <ZoneDrift runs={trackedRuns} />}

      <div className="eyebrow" style={{ marginBottom: 18 }}>
        Last seven days
      </div>
      <div style={{ display: "flex", alignItems: "flex-end", gap: 34, flexWrap: "wrap" }}>
        <div style={{ flex: 1, minWidth: 320 }}>
          <LineChart
            series={[{ values: week }]}
            height={84}
            viewWidth={440}
            pad={6}
          />
          <AxisLabels labels={axis} />
        </div>
        <div style={{ fontSize: 14, lineHeight: 1.6, color: "var(--mut)", maxWidth: "26ch" }}>
          {weekTotal > 0
            ? `${weekTotal.toFixed(1)} km across ${sessions} ${sessions === 1 ? "session" : "sessions"}.`
            : `${sessions} ${sessions === 1 ? "session" : "sessions"}, none with recorded distance.`}{" "}
          {priorWeek > 0 && weekTotal > 0
            ? loadSentence(weekTotal, priorWeek)
            : "No previous week to compare against yet."}
        </div>
      </div>
    </div>
  );
}

/**
 * The last session in full.
 *
 * Today used to mention it in one clause of the opening paragraph — "your last
 * session was 1.2 km treadmill running" — which is the one place you'd expect
 * to find the numbers and the one place they weren't. Zones lead, because
 * that's the metric this app is for; the rest follows as a strip of figures.
 */
function LastSession({ activity }: { activity: CachedActivity }) {
  const when = parseLocal(activity.startTimeLocal ?? activity.localDate);
  const run = isRun(activity.typeKey);
  const secs = activity.movingDurationS ?? activity.durationS;
  const pct = zonePercentages(activity);
  const aboveZ2 = pct[2] + pct[3] + pct[4];

  const stats: [string, string][] = [
    ["Distance", km(activity.distanceM)],
    ["Time", duration(secs)],
    [run ? "Pace" : "Speed", run ? `${pace(activity.distanceM, secs)} /km` : speed(activity.distanceM, secs)],
    ["Avg HR", bpm(activity.avgHr)],
    ["Max HR", bpm(activity.maxHr)],
    // Already both feet, as the rest of the app reports it — doubling it here
    // gave a 263 spm treadmill jog.
    ["Cadence", activity.avgCadence ? `${Math.round(activity.avgCadence)} spm` : DASH],
    ["Aerobic TE", activity.aerobicTe ? activity.aerobicTe.toFixed(1) : DASH],
  ];

  return (
    <>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: 16,
          marginBottom: 16,
        }}
      >
        <div className="eyebrow">Last session</div>
        <Link
          className="underlined"
          to="/activities/$activityId"
          params={{ activityId: String(activity.activityId) }}
          style={{ fontSize: 12.5, whiteSpace: "nowrap" }}
        >
          Full breakdown
        </Link>
      </div>

      <div style={{ fontSize: 17, marginBottom: 4 }}>
        {activity.name || sportLabel(activity.typeKey)}
      </div>
      <div style={{ fontSize: 12.5, color: "var(--faint)", marginBottom: 20 }}>
        {sportLabel(activity.typeKey)}
        {when ? ` · ${shortDate(when)}` : ""}
        {hasZoneData(activity) ? ` · ${aboveZ2.toFixed(0)}% above Z2` : ""}
      </div>

      <ZoneBar activity={activity} />

      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "20px 38px",
          marginTop: 24,
        }}
      >
        {stats.map(([label, value]) => (
          <div key={label}>
            <div className="mono" style={{ fontSize: 19, letterSpacing: "-0.03em" }}>
              {value}
            </div>
            <div
              style={{
                font: "400 10px/1 'Instrument Sans', sans-serif",
                letterSpacing: "0.11em",
                textTransform: "uppercase",
                color: "var(--faint)",
                marginTop: 8,
              }}
            >
              {label}
            </div>
          </div>
        ))}
      </div>

      <Rule m="44px 0 26px" />
    </>
  );
}

/**
 * Every recent run's zone mix side by side, newest last.
 *
 * A single session's split says nothing on its own — the question is whether
 * the hard-effort share is creeping back up, and that only shows as a
 * sequence. Reading right to left, more accent means more drift.
 */
function ZoneDrift({ runs }: { runs: CachedActivity[] }) {
  const oldestFirst = [...runs].reverse();
  const shares = oldestFirst.map((a) => {
    const p = zonePercentages(a);
    return p[2] + p[3] + p[4];
  });

  // Compared against the same 20% the 80/20 model asks for, so the sentence
  // and the reference line can't disagree.
  const TARGET = 20;
  const recent = shares.slice(-3);
  const avgRecent = recent.reduce((a, b) => a + b, 0) / recent.length;

  return (
    <>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          justifyContent: "space-between",
          gap: 16,
          marginBottom: 18,
        }}
      >
        <div className="eyebrow">Zone drift · last {runs.length} runs</div>
        <div style={{ fontSize: 12.5, color: "var(--mut)" }}>
          {avgRecent.toFixed(0)}% above Z2 lately · {TARGET}% is the target
        </div>
      </div>

      <div style={{ display: "flex", gap: 10, alignItems: "flex-end", height: 96 }}>
        {oldestFirst.map((a, i) => {
          const when = parseLocal(a.startTimeLocal ?? a.localDate);
          return (
            <Link
              key={a.activityId}
              to="/activities/$activityId"
              params={{ activityId: String(a.activityId) }}
              title={`${a.name ?? sportLabel(a.typeKey)} — ${shares[i].toFixed(0)}% above Z2`}
              style={{
                flex: 1,
                display: "flex",
                flexDirection: "column",
                justifyContent: "flex-end",
                height: "100%",
                gap: 7,
                color: "inherit",
              }}
            >
              <span className="mono" style={{ fontSize: 11.5, color: "var(--mut)" }}>
                {shares[i].toFixed(0)}%
              </span>
              {/* Full-height track so the bars share a scale and a short run
                  doesn't read as an easy one. */}
              <span style={{ display: "block", flex: 1, position: "relative" }}>
                <span
                  style={{
                    position: "absolute",
                    left: 0,
                    right: 0,
                    bottom: `${TARGET}%`,
                    height: 1,
                    background: "var(--line)",
                  }}
                />
                <span
                  style={{
                    position: "absolute",
                    inset: "auto 0 0 0",
                    height: `${Math.max(shares[i], 1.5)}%`,
                    background: shares[i] > TARGET ? "var(--acc)" : "var(--mut)",
                    borderRadius: "2px 2px 0 0",
                  }}
                />
              </span>
              <span style={{ fontSize: 10.5, color: "var(--faint)", textAlign: "center" }}>
                {when ? shortDate(when) : DASH}
              </span>
            </Link>
          );
        })}
      </div>

      <Rule m="44px 0 26px" />
    </>
  );
}

/** Only ever called with a day that actually has food logged. */
function fuelSentence(
  consumed: number | null,
  burned: number | null,
  balance: number | null,
  date: string,
): string {
  const when = parseLocal(date);
  const day = isoDate(new Date()) === date ? "Today" : when ? shortDate(when) : "That day";
  if (balance == null) {
    return `${day}: ${num(consumed)} kcal logged.`;
  }
  const verb = balance < 0 ? "deficit" : "surplus";
  return `${day}: ${num(consumed)} kcal in against ${num(burned)} burned — a ${num(Math.abs(balance))} kcal ${verb}.`;
}

function countSessions(activities: CachedActivity[], days: number): number {
  const cutoff = new Date();
  cutoff.setDate(cutoff.getDate() - (days - 1));
  const key = cutoff.toISOString().slice(0, 10);
  return activities.filter((a) => (a.localDate ?? "") >= key).length;
}

function loadSentence(now: number, prior: number): string {
  const change = ((now - prior) / prior) * 100;
  const dir = change >= 0 ? "up" : "down";
  const abs = Math.abs(change);
  const verdict =
    abs < 10
      ? "steady"
      : change > 0 && abs > 30
        ? "a big jump for one week"
        : "inside a sensible range";
  return `Distance is ${dir} ${abs.toFixed(0)}% on the previous week — ${verdict}.`;
}

/**
 * The opening paragraph. Assembled from clauses that each only appear when the
 * underlying number exists, so a partial day reads as a shorter sentence
 * rather than one full of dashes.
 */
function narrative(rows: DailyMetrics[], activities: CachedActivity[]): string {
  const parts: string[] = [];

  const sleep = latest(rows, "sleepSecs");
  const sleepAvg = mean(pick(rows, "sleepSecs"));
  if (sleep) {
    const vs =
      sleepAvg == null
        ? ""
        : sleep.value > sleepAvg * 1.05
          ? ", above your recent average"
          : sleep.value < sleepAvg * 0.95
            ? ", short of your recent average"
            : ", about your usual";
    parts.push(`You slept ${hoursMinutes(sleep.value)}${vs}.`);
  }

  const battery = latest(rows, "bodyBatteryHigh");
  const readiness = latest(rows, "trainingReadiness");
  if (battery && readiness) {
    parts.push(
      `Body battery peaked at ${num(battery.value)} and Garmin puts your readiness at ${num(readiness.value)}.`,
    );
  } else if (readiness) {
    parts.push(`Garmin puts your readiness at ${num(readiness.value)}.`);
  }

  const last = activities[0];
  if (last) {
    const when = parseLocal(last.startTimeLocal ?? last.localDate);
    const days = when
      ? Math.round((Date.now() - when.getTime()) / 86_400_000)
      : null;
    const ago =
      days === 0 ? "Today's" : days === 1 ? "Yesterday's" : days ? `${days} days ago, your` : "Your last";
    const dist = last.distanceM ? ` ${km(last.distanceM)}` : "";
    parts.push(
      `${ago} session was${dist ? dist : ""} ${(last.typeKey ?? "activity").replace(/_/g, " ")}.`,
    );
  }

  const runs = activities.filter((a) => isRun(a.typeKey)).slice(0, 5);
  const split = easyHardSplit(runs);
  if (split) {
    parts.push(
      split.hardPct > 40
        ? `Across your last ${split.counted} runs with heart-rate data, ${split.hardPct.toFixed(0)}% of the time was above Z2 — still the thing most worth fixing.`
        : `Across your last ${split.counted} runs with heart-rate data, ${split.easyPct.toFixed(0)}% of the time was in Z1–Z2, which is where you want it.`,
    );
  }

  return parts.join(" ") || "No metrics recorded for the last few days.";
}
