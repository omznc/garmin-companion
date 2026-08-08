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
        consumedKcal: null,
        totalBurnKcal: null,
        activeKcal: null,
        bmrKcal: null,
        netCalorieGoal: null,
        hydrationMl: null,
        hydrationGoalMl: null,
        sweatLossMl: null,
      },
    );
  }
  return out;
}

export const pick = <K extends keyof DailyMetrics>(rows: DailyMetrics[], key: K): Point[] =>
  rows.map((r) => (r[key] as number | null) ?? null);

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

/* ------------------------------------------------------------------ fuel --- */

/** Eaten minus burned for one day. Negative is a deficit, null unless both sides exist. */
export const balanceKcal = (d: DailyMetrics): number | null =>
  d.consumedKcal != null && d.totalBurnKcal != null ? d.consumedKcal - d.totalBurnKcal : null;

export interface Fuel {
  /** The most recent day carrying a food log, or null if the window has none. */
  day: DailyMetrics | null;
  /** 0 when that day is today, 1 for yesterday, and so on. */
  age: number | null;
  /** Days in the window and how many of them were logged. */
  window: number;
  logged: number;
  /** Averaged over logged days only — averaging in blanks as zero would
   *  manufacture a deficit out of a day nobody wrote down. */
  avgBalance: number | null;
  /** Burn comes off the device whether or not food was logged, so this
   *  survives an unlogged week and is what keeps the fuel line on screen. */
  avgBurn: number | null;
}

/**
 * The food side of the last `days`, as the screens need it.
 *
 * Two things this deliberately keeps apart: a day with no food log, and a day
 * of no food. Only the first is common — 61 of the 326 days in this cache
 * carry a log — so every field states which population it was computed over.
 */
export function fuel(rows: DailyMetrics[], days = 7): Fuel {
  const window = rows.slice(-days);
  const logged = window.filter((d) => d.consumedKcal != null);

  // `window` is oldest-first and padded to today, so the distance from the end
  // is the age in days without having to parse a date back out.
  const lastIdx = window.map((d) => d.consumedKcal != null).lastIndexOf(true);
  const day = lastIdx === -1 ? null : window[lastIdx];

  const balances = logged.map(balanceKcal).filter((v): v is number => v != null);
  const burns = window.map((d) => d.totalBurnKcal).filter((v): v is number => v != null);

  return {
    day,
    age: lastIdx === -1 ? null : window.length - 1 - lastIdx,
    window: window.length,
    logged: logged.length,
    avgBalance: balances.length ? balances.reduce((a, b) => a + b, 0) / balances.length : null,
    avgBurn: burns.length ? burns.reduce((a, b) => a + b, 0) / burns.length : null,
  };
}

/* ------------------------------------------------------------ hydration --- */

/**
 * Hydration in ml, with zero read as "not logged".
 *
 * Garmin writes a 0 on days nothing tracked hydration rather than leaving the
 * field null, and a person who drank nothing at all for a day is not a case
 * worth modelling. Stripping the zeros here rather than at each call site is
 * what makes the rest of the app behave: `hasData` stops reporting an all-zero
 * column as populated, `mean` stops averaging blanks into a number, and the
 * charts stop drawing a flat line along the floor.
 */
export const hydrationMl = (d: DailyMetrics): number | null =>
  d.hydrationMl != null && d.hydrationMl > 0 ? d.hydrationMl : null;

/**
 * Sweat loss in ml, zeros stripped the same way.
 *
 * Unlike intake this one is usually real, because Garmin computes it from the
 * session rather than waiting for anyone to log it — so an account with an
 * empty hydration column can still have a full sweat column, and the two have
 * to be asked about separately.
 */
export const sweatLossMl = (d: DailyMetrics): number | null =>
  d.sweatLossMl != null && d.sweatLossMl > 0 ? d.sweatLossMl : null;

export interface Hydration {
  /** Days in the window carrying a real reading, and the window itself. */
  logged: number;
  window: number;
  avgMl: number;
  /** The most recent reading, with 0 meaning today. */
  latest: { ml: number; age: number } | null;
  /** Garmin's own daily target, where the account sets one. */
  goalMl: number | null;
}

/**
 * The hydration side of the last `days`, or null when this account doesn't
 * track it — which is the common case, and the reason nothing may assume the
 * column is meaningful just because rows exist.
 */
export function hydration(rows: DailyMetrics[], days = 7): Hydration | null {
  const window = rows.slice(-days);
  const values = window.map(hydrationMl);
  const real = values.filter((v): v is number => v != null);
  if (!real.length) return null;

  const lastIdx = values.map((v) => v != null).lastIndexOf(true);
  const goals = window.map((d) => d.hydrationGoalMl).filter((v): v is number => v != null && v > 0);

  return {
    logged: real.length,
    window: window.length,
    avgMl: real.reduce((a, b) => a + b, 0) / real.length,
    latest: lastIdx === -1 ? null : { ml: values[lastIdx]!, age: values.length - 1 - lastIdx },
    goalMl: goals.length ? goals[goals.length - 1] : null,
  };
}

/* ----------------------------------------------------------------- zones --- */

export const zoneTotal = (a: CachedActivity) => a.zoneSecs.reduce((x, y) => x + y, 0);

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
  /** What the sparkline's numbers are, for its hover readout. */
  sparkUnit?: string;
  link?: { label: string; to: string };
}

/**
 * The "Attention" list. Each entry is a threshold crossed by real numbers, and
 * every one states the comparison it made so the claim can be checked.
 */
export function attention(daily: DailyMetrics[], activities: CachedActivity[]): Observation[] {
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
        sparkUnit: "bpm",
        link: { label: "Ask about this", to: "/ask" },
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
      sparkUnit: "ms",
    });
  }

  // Zone drift: the thing this whole app exists to watch.
  const runs = activities.filter((a) => isRun(a.typeKey)).slice(0, 5);
  const split = easyHardSplit(runs);
  if (split && split.hardPct > 50 && split.counted >= 3) {
    out.push({
      accent: true,
      text: `${split.hardPct.toFixed(0)}% of your last ${split.counted} runs with heart-rate data was above Z2. The 80/20 target is 20%.`,
      link: { label: "Insights", to: "/insights" },
    });
  }

  const lowCadence = runs.filter((a) => a.avgCadence != null && a.avgCadence < 155);
  if (lowCadence.length >= 3) {
    const avg = mean(lowCadence.map((a) => a.avgCadence));
    out.push({
      accent: false,
      text: `Cadence is averaging ${avg?.toFixed(0)} spm across your recent runs. Quicker, lighter steps nearer 170 cut joint load.`,
    });
  }

  // Fuelling, but only where the log is dense enough to mean anything. Three
  // logged days out of seven is a sample, not a week, and a deficit computed
  // off one logged day is an accident of which day got written down.
  const f = fuel(daily, 7);
  if (f.avgBalance != null && f.logged >= 3) {
    const over = f.logged === f.window ? "" : ` across the ${f.logged} of 7 days you logged`;
    if (f.avgBalance <= -500) {
      out.push({
        accent: true,
        text: `You're averaging a ${Math.abs(f.avgBalance).toFixed(0)} kcal daily deficit${over}. That's a big gap to train on — under-fuelling shows up as a stalled easy pace and a resting heart rate that won't settle.`,
        link: { label: "Food", to: "/food" },
      });
    } else if (f.avgBalance >= 700) {
      out.push({
        accent: false,
        text: `You're averaging a ${f.avgBalance.toFixed(0)} kcal daily surplus${over}. Worth knowing rather than worth fixing — at ${(f.avgBurn ?? 0).toFixed(0)} kcal burned a day it's a choice, not a slip.`,
        link: { label: "Food", to: "/food" },
      });
    }
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

/** A charted side of a correlation, named and unitted so hovering the chart
 *  says which line is which and what its numbers are. */
export interface InsightSeries {
  name: string;
  values: Point[];
  format: (v: number) => string;
}

export interface Insight {
  claim: string;
  detail: string;
  basis: string;
  a: InsightSeries;
  b: InsightSeries;
}

/** Units for the charted pairs. Each series carries its own — the two sides of
 *  a correlation are never in the same one. */
const UNIT = {
  hours: (v: number) => `${v.toFixed(1)} h`,
  bpm: (v: number) => `${v.toFixed(0)} bpm`,
  ms: (v: number) => `${v.toFixed(0)} ms`,
  kcal: (v: number) => `${v > 0 ? "+" : ""}${v.toFixed(0)} kcal`,
  spm: (v: number) => `${v.toFixed(0)} spm`,
  score: (v: number) => v.toFixed(0),
  pace: (v: number) => {
    const m = Math.floor(v);
    const s = Math.round((v - m) * 60);
    return s === 60 ? `${m + 1}:00 /km` : `${m}:${String(s).padStart(2, "0")} /km`;
  },
};

/**
 * Correlations mined from the cache. Every one reports its sample size and
 * coefficient, and none is emitted below the minimum pair count — a
 * correlation over five points is a coincidence with a decimal place.
 */
export function insights(daily: DailyMetrics[], activities: CachedActivity[]): Insight[] {
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
      a: { name: "Sleep", values: sleep, format: UNIT.hours },
      b: { name: "Resting HR", values: rhr, format: UNIT.bpm },
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
      a: { name: "HRV", values: hrv, format: UNIT.ms },
      b: { name: "Readiness", values: readiness, format: UNIT.score },
    });
  }

  // Energy balance against the next morning's resting heart rate. Lagged the
  // same way as sleep: what you ate today is scored by tomorrow's morning
  // reading, not by the one taken before you'd eaten it.
  const balance = daily.map(balanceKcal);
  const balanceVsRhr = correlation(balance.slice(0, -1), rhr.slice(1), 14);
  if (balanceVsRhr && Math.abs(balanceVsRhr.r) > 0.2) {
    const eating = balanceVsRhr.r < 0 ? "eating more" : "eating less";
    out.push({
      claim: "What you eat lands on the next morning's resting heart rate.",
      detail: `Over ${balanceVsRhr.n} days where you logged food, ${eating} than you burned goes with a lower resting heart rate the following morning (r = ${balanceVsRhr.r.toFixed(2)}). Read it as a hint rather than a finding — this is the sparsest series in your cache, because most days carry no food log at all.`,
      basis: `${balanceVsRhr.n} logged days · r = ${balanceVsRhr.r.toFixed(2)}`,
      a: { name: "Balance", values: balance, format: UNIT.kcal },
      b: { name: "Resting HR", values: rhr, format: UNIT.bpm },
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
        a: { name: "Cadence", values: cadence, format: UNIT.spm },
        b: { name: "Pace", values: paceMin, format: UNIT.pace },
      });
    }
  }

  return out;
}
