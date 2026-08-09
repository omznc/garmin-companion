/**
 * What's left of the Insights analyses after the findings moved to Rust.
 *
 * The seven deep findings — fitness at a fixed heart rate, the cadence lever,
 * the recovery drivers, the easy share, the week's shape, the rest-day contrast
 * — used to live here, and the coach you asked about them had no idea they
 * existed. They are now `garmin_core::findings`, computed once and read by the
 * Insights screen, the MCP server and the app's chat alike, per the rule in
 * CLAUDE.md. Keeping a second copy here would have been a second truth: the
 * Rust versions gate on a bootstrap interval that excludes zero and these
 * didn't, so the two would have disagreed about what is worth saying.
 *
 * What stays is what nothing else wanted: the weekly load shape, and the null
 * checks — the comparisons that came back empty, which are worth a screen of
 * their own and have no claim to make anywhere else.
 *
 * The rules are unchanged. Nothing is invented, every finding carries the
 * sample it was computed over, and a function returns null rather than a hedged
 * sentence when the data won't support the claim.
 */
import type { CachedActivity, DailyMetrics } from "./api";
import { correlation, type Point } from "./chart";
import { isoDate, parseLocal } from "./format";

/* ------------------------------------------------------------------ types --- */

/**
 * The heading a finding is filed under. Rendered in this order.
 *
 * The finding shape itself is `ApiFinding` in `api.ts` now, mirroring the Rust.
 * This stays because it is the screen's section order, which is a property of
 * the layout rather than of the analysis, and because the strings have to agree
 * with `garmin_core::findings::Section` — they are matched, not translated.
 */
export type Section = "Fitness" | "Recovery" | "Patterns";

export const SECTIONS: Section[] = ["Fitness", "Recovery", "Patterns"];

/* ---------------------------------------------------------------- helpers --- */

const HOUR = 3600;

/** Seconds above threshold — Z4 and Z5. Z3 is tempo, which is not "hard". */
export const hardSecs = (a: CachedActivity) => a.zoneSecs[3] + a.zoneSecs[4];

/** Five minutes above threshold in a day. Below that it's a warm-up spike. */
const HARD_DAY_SECS = 300;

const avg = (xs: number[]) => xs.reduce((a, b) => a + b, 0) / xs.length;

const shiftDate = (iso: string, days: number): string => {
  const d = parseLocal(iso);
  if (!d) return iso;
  d.setDate(d.getDate() + days);
  return isoDate(d);
};

/**
 * Days the watch was actually worn.
 *
 * `dailySeries` pads its window to a continuous run of calendar days, so a
 * blank row is indistinguishable from a day off unless you ask. Every rate in
 * this file — sessions per Tuesday, nights below baseline — divides by this,
 * never by the padded length, or a month of unsynced history would read as a
 * month of not training.
 */
const observed = (daily: DailyMetrics[]) =>
  daily.filter((d) => d.steps != null || d.restingHr != null || d.sleepSecs != null);

/** Activities grouped by the local date they happened on. */
function byDate(activities: CachedActivity[]): Map<string, CachedActivity[]> {
  const out = new Map<string, CachedActivity[]>();
  for (const a of activities) {
    if (!a.localDate) continue;
    const list = out.get(a.localDate);
    if (list) list.push(a);
    else out.set(a.localDate, [a]);
  }
  return out;
}

/**
 * Edwards' training impulse: minutes in each heart-rate zone, weighted one
 * through five. One unit of load that knows an hour of Z2 and an hour of Z4 are
 * not the same hour, which raw duration doesn't.
 */
export function edwardsTrimp(a: CachedActivity): number {
  const total = a.zoneSecs.reduce((x, y) => x + y, 0);
  // No heart rate recorded — most strength sessions here. Counting it as zero
  // would make a lifting week read as a rest week; Z2 is the fair guess.
  if (total <= 0) return ((a.durationS ?? 0) / 60) * 2;
  return a.zoneSecs.reduce((t, secs, i) => t + (secs / 60) * (i + 1), 0);
}

/* ------------------------------------------------------------------ load --- */

export interface WeekLoad {
  /** Monday, `YYYY-MM-DD`. */
  start: string;
  /** Edwards TRIMP: minutes in each zone weighted 1–5. */
  trimp: number;
  /** Weekly mean daily load over its own standard deviation. */
  monotony: number | null;
  strain: number | null;
}

export interface LoadShape {
  weeks: WeekLoad[];
  /** The most recent complete week — the one the numbers describe. */
  latest: WeekLoad;
  /** Weeks in the window whose monotony cleared Foster's 2.0. */
  monotonous: number;
}

/**
 * Weekly training impulse, and the shape of the week that produced it.
 *
 * Load alone is a poor predictor of who gets hurt; load *distributed evenly*
 * is a better one. Foster's monotony is the week's mean daily load over its own
 * standard deviation — high when every day is the same, which is the pattern
 * that grinds — and strain is the week's load multiplied by it. Both need a
 * daily load figure that knows the difference between an hour of Z2 and an hour
 * of Z4, so this uses Edwards' TRIMP: minutes in each zone, weighted 1 to 5.
 */
export function loadShape(activities: CachedActivity[], weeks = 12): LoadShape | null {
  const buckets = new Map<string, number[]>();
  for (const a of activities) {
    const d = parseLocal(a.startTimeLocal ?? a.localDate);
    if (!d) continue;
    const start = new Date(d.getFullYear(), d.getMonth(), d.getDate());
    start.setDate(start.getDate() - ((start.getDay() + 6) % 7));
    const key = isoDate(start);
    const daysOf = buckets.get(key) ?? new Array(7).fill(0);
    daysOf[(d.getDay() + 6) % 7] += edwardsTrimp(a);
    buckets.set(key, daysOf);
  }

  const ordered = [...buckets.entries()]
    .sort((a, b) => (a[0] < b[0] ? -1 : 1))
    .slice(-weeks)
    .map(([start, load]): WeekLoad => {
      const m = avg(load);
      const sd = Math.sqrt(avg(load.map((v) => (v - m) ** 2)));
      const trimp = load.reduce((x, y) => x + y, 0);
      const monotony = sd > 0 ? m / sd : null;
      return { start, trimp, monotony, strain: monotony == null ? null : trimp * monotony };
    });
  if (ordered.length < 4) return null;

  return {
    weeks: ordered,
    latest: ordered[ordered.length - 1],
    monotonous: ordered.filter((w) => (w.monotony ?? 0) >= 2).length,
  };
}

/* ------------------------------------------------------------ null results --- */

/**
 * A question the data was asked that came back with nothing.
 *
 * Worth showing. Most of these are things it would be reasonable to believe —
 * that sleeping longer lifts your HRV, that training late costs you sleep — and
 * an app that only ever reports the comparisons that worked is quietly telling
 * you the ones that didn't were never run.
 */
export interface NullCheck {
  question: string;
  verdict: string;
  basis: string;
}

export function nullChecks(daily: DailyMetrics[], activities: CachedActivity[]): NullCheck[] {
  const rows = observed(daily);
  const out: NullCheck[] = [];
  const days = byDate(activities);
  const index = new Map(rows.map((d) => [d.date, d]));

  const weakCorrelation = (xs: Point[], ys: Point[], question: string, verdict: string) => {
    const c = correlation(xs, ys, 20);
    // A strong result here is a finding, not a null. It gets dropped rather
    // than reported, because somewhere above is where it belongs.
    if (!c || Math.abs(c.r) >= 0.2) return;
    out.push({ question, verdict, basis: `${c.n} paired days · r = ${c.r.toFixed(2)}` });
  };

  // Sleep, readiness and HRV are all recorded against the morning you woke up
  // on, so a night is a single row and the pairing is same-index. Lagging by
  // one would be asking whether tonight's sleep affects tomorrow night.
  const sleep = rows.map((d) => (d.sleepSecs == null ? null : d.sleepSecs / HOUR));
  const readiness = rows.map((d) => d.trainingReadiness);
  const steps = rows.map((d) => d.steps);
  const stress = rows.map((d) => d.stressAvg);

  weakCorrelation(
    sleep,
    readiness,
    "Does a long night buy you a better readiness score?",
    "Not measurably. Across the range you actually sleep — roughly five to nine hours — length alone barely moves the score, which is being driven by something else about the night.",
  );
  weakCorrelation(
    steps,
    stress,
    "Do the days you move more come out less stressed?",
    "No. Your step count and your stress average are close to independent of each other.",
  );

  // Sessions that finish late, against that night's sleep.
  const late: number[] = [];
  const early: number[] = [];
  for (const [date, list] of days) {
    const next = index.get(shiftDate(date, 1));
    if (!next || next.sleepSecs == null) continue;
    const finish = list
      .map((a) => {
        const start = parseLocal(a.startTimeLocal);
        return start ? start.getHours() + (a.durationS ?? 0) / HOUR : null;
      })
      .filter((h): h is number => h != null);
    if (!finish.length) continue;
    (Math.max(...finish) >= 20 ? late : early).push(next.sleepSecs / HOUR);
  }
  if (late.length >= 15 && early.length >= 15) {
    const gap = avg(early) - avg(late);
    if (Math.abs(gap) < 0.5) {
      out.push({
        question: "Do late sessions cost you sleep?",
        verdict: `Barely. Nights after a session finishing past 20:00 ran ${(Math.abs(gap) * 60).toFixed(0)} minutes ${gap > 0 ? "shorter" : "longer"} — inside the spread of any other night.`,
        basis: `${late.length} late nights against ${early.length} earlier ones`,
      });
    }
  }

  // Compensatory inactivity: the day after a hard session, do you move less?
  const stepsAfter = (dates: string[]) =>
    dates.map((d) => index.get(shiftDate(d, 1))?.steps).filter((s): s is number => s != null);
  const hardDates = [...days.keys()].filter(
    (d) => days.get(d)!.reduce((t, a) => t + hardSecs(a), 0) > HARD_DAY_SECS,
  );
  const restDates = rows.filter((d) => !days.has(d.date)).map((d) => d.date);
  const afterHard = stepsAfter(hardDates);
  const afterRest = stepsAfter(restDates);
  if (afterHard.length >= 20 && afterRest.length >= 20) {
    const gap = avg(afterRest) - avg(afterHard);
    if (Math.abs(gap) < 1200) {
      out.push({
        question: "Do you move less on the day after a hard session?",
        verdict: `Only just — ${Math.abs(gap).toFixed(0)} steps ${gap > 0 ? "fewer" : "more"} than the day after a rest day. The compensation effect that shows up in the literature isn't showing up in you.`,
        basis: `${afterHard.length} days after hard sessions against ${afterRest.length} after rest days`,
      });
    }
  }

  return out;
}
