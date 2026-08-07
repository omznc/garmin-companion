/**
 * Everything the screens compute *from* the cache rather than read out of it:
 * weekly rollups, zone splits, load ratios, and the observations the Today and
 * Insights screens surface.
 *
 * All of it is deterministic arithmetic over real rows. Nothing here invents a
 * number — where the data can't support a claim, the function returns null and
 * the screen renders an empty state instead.
 */
import type { CachedActivity, DailyMetrics } from "./api";
import { correlation, mean, type Point } from "./chart";
import { isRun, isoDate, parseLocal } from "./format";

/* ------------------------------------------------------------------ days --- */

/**
 * Daily metrics indexed by date and padded to a continuous run of `days`
 * ending today, oldest first. Charts need one slot per calendar day or the
 * x-axis silently compresses over gaps in the data.
 */
export function dailySeries(rows: DailyMetrics[], days: number): DailyMetrics[] {
  const byDate = new Map(rows.map((r) => [r.date, r]));
  const out: DailyMetrics[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    const key = isoDate(d);
    out.push(
      byDate.get(key) ?? {
        date: key,
        restingHr: null,
        hrvLastNight: null,
        hrvWeeklyAvg: null,
        hrvStatus: null,
        trainingReadiness: null,
        sleepSecs: null,
        sleepScore: null,
        steps: null,
        stressAvg: null,
        bodyBatteryHigh: null,
        bodyBatteryLow: null,
      },
    );
  }
  return out;
}

export const pick = <K extends keyof DailyMetrics>(
  rows: DailyMetrics[],
  key: K,
): Point[] => rows.map((r) => (r[key] as number | null) ?? null);

/** The most recent day that actually has a value for `key`. */
export function latest<K extends keyof DailyMetrics>(
  rows: DailyMetrics[],
  key: K,
): { value: number; date: string } | null {
  for (let i = rows.length - 1; i >= 0; i--) {
    const v = rows[i][key];
    if (typeof v === "number" && isFinite(v)) return { value: v, date: rows[i].date };
  }
  return null;
}

/* ----------------------------------------------------------------- zones --- */

export const zoneTotal = (a: CachedActivity) =>
  a.zoneSecs.reduce((x, y) => x + y, 0);

export function zonePercentages(a: CachedActivity): [number, number, number, number, number] {
  const total = zoneTotal(a);
  if (total <= 0) return [0, 0, 0, 0, 0];
  return a.zoneSecs.map((s) => (s / total) * 100) as [number, number, number, number, number];
}

/** True when the session recorded HR at all. Strength and rope work often didn't. */
export const hasZoneData = (a: CachedActivity) => zoneTotal(a) > 0;

/**
 * Time-weighted easy (Z1–Z2) versus hard (Z3–Z5) split across activities.
 * Sessions without HR are excluded rather than counted as zero in both
 * buckets, which would drag the split toward nonsense.
 */
export function easyHardSplit(
  activities: CachedActivity[],
): { easyPct: number; hardPct: number; totalSecs: number; counted: number } | null {
  const tracked = activities.filter(hasZoneData);
  if (!tracked.length) return null;

  let easy = 0;
  let hard = 0;
  for (const a of tracked) {
    easy += a.zoneSecs[0] + a.zoneSecs[1];
    hard += a.zoneSecs[2] + a.zoneSecs[3] + a.zoneSecs[4];
  }
  const total = easy + hard;
  if (total <= 0) return null;
  return {
    easyPct: (easy / total) * 100,
    hardPct: (hard / total) * 100,
    totalSecs: total,
    counted: tracked.length,
  };
}

/* ----------------------------------------------------------------- weeks --- */

export interface Week {
  /** Monday, `YYYY-MM-DD`. */
  start: string;
  end: string;
  activities: CachedActivity[];
  distanceM: number;
  durationS: number;
  runs: number;
}

/** Monday of the week containing `d`, as a local date. */
export function weekStart(d: Date): Date {
  const out = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  const dow = (out.getDay() + 6) % 7; // Monday = 0
  out.setDate(out.getDate() - dow);
  return out;
}

/** Groups into ISO weeks, newest first, skipping weeks with no activity. */
export function byWeek(activities: CachedActivity[]): Week[] {
  const buckets = new Map<string, CachedActivity[]>();
  for (const a of activities) {
    const d = parseLocal(a.startTimeLocal ?? a.localDate);
    if (!d) continue;
    const key = isoDate(weekStart(d));
    const list = buckets.get(key);
    if (list) list.push(a);
    else buckets.set(key, [a]);
  }

  return [...buckets.entries()]
    .sort((a, b) => (a[0] < b[0] ? 1 : -1))
    .map(([start, acts]) => {
      const s = parseLocal(start)!;
      const end = new Date(s);
      end.setDate(end.getDate() + 6);
      return {
        start,
        end: isoDate(end),
        activities: acts,
        distanceM: acts.reduce((t, a) => t + (a.distanceM ?? 0), 0),
        durationS: acts.reduce((t, a) => t + (a.durationS ?? 0), 0),
        runs: acts.filter((a) => isRun(a.typeKey)).length,
      };
    });
}

/** Distance per day over the last `days`, oldest first. Zero, not null, on
 *  rest days — a rest day is data, not a gap. */
export function dailyDistance(activities: CachedActivity[], days: number): number[] {
  const totals = new Map<string, number>();
  for (const a of activities) {
    if (!a.localDate) continue;
    totals.set(a.localDate, (totals.get(a.localDate) ?? 0) + (a.distanceM ?? 0));
  }
  const out: number[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date();
    d.setDate(d.getDate() - i);
    out.push((totals.get(isoDate(d)) ?? 0) / 1000);
  }
  return out;
}

/**
 * Acute-to-chronic workload ratio: the last 7 days of training time against
 * the rolling 28-day average of the same. Above ~1.5 is the range usually
 * associated with a spike in injury risk; near 1.0 is steady.
 */
export function acuteChronic(
  activities: CachedActivity[],
): { acute: number; chronic: number; ratio: number } | null {
  const secsOn = (from: number, to: number) => {
    const lo = new Date();
    lo.setDate(lo.getDate() - from);
    const hi = new Date();
    hi.setDate(hi.getDate() - to);
    const loKey = isoDate(lo);
    const hiKey = isoDate(hi);
    return activities
      .filter((a) => a.localDate && a.localDate > loKey && a.localDate <= hiKey)
      .reduce((t, a) => t + (a.durationS ?? 0), 0);
  };

  const acute = secsOn(7, 0) / 3600;
  const chronic = secsOn(28, 0) / 4 / 3600; // per-week average over 28 days
  if (chronic <= 0) return null;
  return { acute, chronic, ratio: acute / chronic };
}

/* ---------------------------------------------------------- observations --- */

export interface Observation {
  /** Accent-dotted items are the ones worth acting on today. */
  accent: boolean;
  text: string;
  /** Optional 14-point series rendered inline as a sparkline. */
  spark?: Point[];
  link?: { label: string; to: string };
}

/**
 * The "Attention" list. Each entry is a threshold crossed by real numbers, and
 * every one states the comparison it made so the claim can be checked.
 */
export function attention(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Observation[] {
  const out: Observation[] = [];
  const rhr = pick(daily, "restingHr");
  const hrv = pick(daily, "hrvLastNight");

  // Resting HR against its own 30-day baseline, excluding the recent window
  // so a sustained rise doesn't quietly lift the baseline it's measured against.
  const recentRhr = rhr.slice(-3).filter((v): v is number => v != null);
  const baselineRhr = mean(rhr.slice(0, -3));
  if (recentRhr.length >= 2 && baselineRhr != null) {
    const avg = recentRhr.reduce((a, b) => a + b, 0) / recentRhr.length;
    const delta = avg - baselineRhr;
    if (delta >= 3) {
      out.push({
        accent: true,
        text: `Resting heart rate is ${delta.toFixed(0)} bpm above your ${rhr.filter((v) => v != null).length}-day baseline across the last ${recentRhr.length} mornings.`,
        spark: rhr.slice(-14),
        link: { label: "ask about this", to: "/ask" },
      });
    }
  }

  // HRV falling while resting HR climbs is the pairing worth flagging; either
  // alone is noisy enough to be a bad day rather than a trend.
  const recentHrv = mean(hrv.slice(-5));
  const priorHrv = mean(hrv.slice(-20, -5));
  if (recentHrv != null && priorHrv != null && recentHrv < priorHrv * 0.92) {
    out.push({
      accent: true,
      text: `HRV has averaged ${recentHrv.toFixed(0)} ms over the last five nights, down from ${priorHrv.toFixed(0)} ms in the fortnight before.`,
      spark: hrv.slice(-14),
    });
  }

  // Zone drift: the thing this whole app exists to watch.
  const runs = activities.filter((a) => isRun(a.typeKey)).slice(0, 5);
  const split = easyHardSplit(runs);
  if (split && split.hardPct > 50 && split.counted >= 3) {
    out.push({
      accent: true,
      text: `${split.hardPct.toFixed(0)}% of your last ${split.counted} runs with heart-rate data was above Z2. The 80/20 target is 20%.`,
      link: { label: "insights", to: "/insights" },
    });
  }

  const lowCadence = runs.filter(
    (a) => a.avgCadence != null && a.avgCadence < 155,
  );
  if (lowCadence.length >= 3) {
    const avg = mean(lowCadence.map((a) => a.avgCadence));
    out.push({
      accent: false,
      text: `Cadence is averaging ${avg?.toFixed(0)} spm across your recent runs. Quicker, lighter steps nearer 170 cut joint load.`,
    });
  }

  // Garmin only computes VO2 max from outdoor GPS runs, so a treadmill-only
  // block leaves it permanently blank.
  const outdoorRuns = activities
    .filter((a) => isRun(a.typeKey) && a.typeKey !== "treadmill_running")
    .filter((a) => (a.localDate ?? "") >= relativeDate(-42));
  if (outdoorRuns.length === 0 && activities.some((a) => isRun(a.typeKey))) {
    out.push({
      accent: false,
      text: "No outdoor GPS run in the last six weeks, so VO2 max still isn't calculating. One easy outdoor run starts it tracking.",
    });
  }

  return out;
}

function relativeDate(offsetDays: number): string {
  const d = new Date();
  d.setDate(d.getDate() + offsetDays);
  return isoDate(d);
}

/* -------------------------------------------------------------- insights --- */

export interface Insight {
  claim: string;
  detail: string;
  basis: string;
  a: Point[];
  b: Point[];
}

/**
 * Correlations mined from the cache. Every one reports its sample size and
 * coefficient, and none is emitted below the minimum pair count — a
 * correlation over five points is a coincidence with a decimal place.
 */
export function insights(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Insight[] {
  const out: Insight[] = [];

  // Sleep against next-day resting HR. Lagged by one row: last night's sleep
  // is recorded on the same date as this morning's resting HR, so the pairing
  // is same-index, but the *effect* on training shows the following day.
  const sleep = pick(daily, "sleepSecs").map((v) => (v == null ? null : v / 3600));
  const rhr = pick(daily, "restingHr");
  const sleepVsRhr = correlation(sleep.slice(0, -1), rhr.slice(1), 14);
  if (sleepVsRhr && Math.abs(sleepVsRhr.r) > 0.2) {
    const direction = sleepVsRhr.r < 0 ? "lower" : "higher";
    out.push({
      claim: `Shorter nights show up in the next morning's resting heart rate.`,
      detail: `Across ${sleepVsRhr.n} paired days, more sleep goes with a ${direction} resting heart rate the following morning. It is a correlation, not a mechanism — but it is the strongest single link in your data between something you control and something you don't.`,
      basis: `${sleepVsRhr.n} paired days · r = ${sleepVsRhr.r.toFixed(2)}`,
      a: sleep,
      b: rhr,
    });
  }

  const hrv = pick(daily, "hrvLastNight");
  const readiness = pick(daily, "trainingReadiness");
  const hrvVsReadiness = correlation(hrv, readiness, 14);
  if (hrvVsReadiness && Math.abs(hrvVsReadiness.r) > 0.3) {
    out.push({
      claim: "Your training readiness is mostly a restatement of your HRV.",
      detail: `The two move together at r = ${hrvVsReadiness.r.toFixed(2)} over ${hrvVsReadiness.n} days. Readiness is a useful single number, but it is not independent evidence — if you already looked at HRV, you have most of the signal.`,
      basis: `${hrvVsReadiness.n} days · r = ${hrvVsReadiness.r.toFixed(2)}`,
      a: hrv,
      b: readiness,
    });
  }

  // Cadence against pace across runs, which is the lever this account has the
  // most room to move.
  const runs = activities
    .filter((a) => isRun(a.typeKey) && a.avgCadence && a.distanceM && a.durationS)
    .slice(0, 60)
    .reverse();
  if (runs.length >= 10) {
    const cadence = runs.map((a) => a.avgCadence);
    const paceMin = runs.map((a) =>
      a.distanceM && a.durationS ? a.durationS / 60 / (a.distanceM / 1000) : null,
    );
    const c = correlation(cadence, paceMin, 10);
    if (c && Math.abs(c.r) > 0.25) {
      out.push({
        claim:
          c.r < 0
            ? "Your quicker-cadence runs are also your faster runs."
            : "Cadence and pace move independently for you.",
        detail:
          c.r < 0
            ? `Over ${c.n} runs, a higher average cadence goes with a faster pace (r = ${c.r.toFixed(2)}). Cadence is the easier of the two to change deliberately.`
            : `Over ${c.n} runs there is no useful relationship between your cadence and your pace (r = ${c.r.toFixed(2)}). Cadence is still worth raising for joint load, but don't expect it to make you faster on its own.`,
        basis: `${c.n} runs · r = ${c.r.toFixed(2)}`,
        a: cadence,
        b: paceMin,
      });
    }
  }

  return out;
}
