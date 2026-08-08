/**
 * The deeper Insights analyses — the ones that need more than a Pearson r
 * between two daily columns.
 *
 * `derive.ts` holds the arithmetic every screen shares. This holds the work
 * that only the Insights screen does: controlling for effort before comparing
 * two runs, pairing a session against the morning that followed it, splitting
 * a year by weekday, and reading a training block's shape rather than its size.
 *
 * The rules are the same as everywhere else in this app. Nothing is invented,
 * every finding carries the sample it was computed over, and a function returns
 * null rather than a hedged sentence when the data won't support the claim. A
 * finding that reads well and is built on nine data points is worse than no
 * finding, because it will be believed.
 */
import type { CachedActivity, DailyMetrics } from "./api";
import { correlation, mean, type Point } from "./chart";
import { isRun, isoDate, parseLocal, shortDate, sportLabel } from "./format";

/* ------------------------------------------------------------------ types --- */

/** Not a severity. A finding can be remarkable without anything being wrong. */
export type Tone = "good" | "note" | "watch";

/** The heading a finding is filed under. Rendered in this order. */
export type Section = "Fitness" | "Recovery" | "Patterns";

export const SECTIONS: Section[] = ["Fitness", "Recovery", "Patterns"];

export interface FindingSeries {
  name: string;
  values: Point[];
  format: (v: number) => string;
  /** Drawn as the comparison line rather than the subject — thin and dashed. */
  muted?: boolean;
  /** Low values at the top. Pace is the case: smaller is faster. */
  invert?: boolean;
}

/** A row of the small table some findings carry instead of, or under, a chart. */
export interface FindingRow {
  label: string;
  value: string;
  /** The sample behind this row, set beside it in the faint colour. */
  note?: string;
  accent?: boolean;
}

export interface Finding {
  /** Stable slug. Nothing branches on it yet; it keeps keys off array indices. */
  kind: string;
  section: Section;
  tone: Tone;
  /** The sentence, set large. One claim, no hedging — the hedge goes in `basis`. */
  claim: string;
  detail: string;
  /** What was counted. Always present: a claim without one isn't shippable. */
  basis: string;
  series?: FindingSeries[];
  labels?: string[];
  rows?: FindingRow[];
}

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

const daysBetween = (a: string, b: string): number => {
  const x = parseLocal(a);
  const y = parseLocal(b);
  if (!x || !y) return 0;
  return Math.round((y.getTime() - x.getTime()) / 86_400_000);
};

/** "8:34", from decimal minutes per kilometre. */
const paceText = (minPerKm: number): string => {
  const m = Math.floor(minPerKm);
  const s = Math.round((minPerKm - m) * 60);
  return s === 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, "0")}`;
};

const signed = (v: number, digits = 1) =>
  `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(v).toFixed(digits)}`;

/** "06 Aug", for a chart whose points are sessions rather than days. */
const dayLabel = (iso: string) => {
  const d = parseLocal(iso);
  return d ? shortDate(d) : iso;
};

const MONTH_SHORT = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/** `2026-08` → "Aug". The year is carried by the basis line, not by every tick. */
const monthShort = (key: string) => MONTH_SHORT[Number(key.slice(5, 7)) - 1] ?? key;

/** How each charted series writes its own numbers in the hover readout. */
const UNIT = {
  spm: (v: number) => `${v.toFixed(0)} spm`,
  score: (v: number) => v.toFixed(0),
  pct: (v: number) => `${v.toFixed(0)}%`,
  pace: (v: number) => `${paceText(v)} /km`,
  perBeat: (v: number) => `${v.toFixed(2)} m/beat`,
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
 * Metres covered per heartbeat.
 *
 * The one number that puts a 6-minute sprint and a 15-minute jog on the same
 * axis: distance divided by the beats it cost. It is not effort-independent —
 * you cover more ground per beat when you run harder, up to a point — which is
 * why the fitness finding below controls for heart rate directly rather than
 * trusting this on its own.
 */
const metresPerBeat = (a: CachedActivity): number | null =>
  a.distanceM && a.durationS && a.avgHr ? a.distanceM / (a.avgHr * (a.durationS / 60)) : null;

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

/** Runs long enough to say anything about. A 70-metre entry is a false start. */
const MIN_RUN_M = 400;
const MIN_RUN_S = 240;

function scoredRuns(activities: CachedActivity[]): CachedActivity[] {
  return activities
    .filter(
      (a) =>
        isRun(a.typeKey) &&
        a.localDate != null &&
        (a.distanceM ?? 0) >= MIN_RUN_M &&
        (a.durationS ?? 0) >= MIN_RUN_S &&
        (a.avgHr ?? 0) >= 100,
    )
    .sort((x, y) => (x.localDate! < y.localDate! ? -1 : 1));
}

/* ---------------------------------------------------------------- fitness --- */

/** Half-width of the heart-rate window two runs have to share to be compared. */
const HR_BAND = 8;

/**
 * Pace at a fixed heart rate — this account's substitute for a VO2 max.
 *
 * Garmin will not compute VO2 max without outdoor GPS runs, and nearly every
 * run here is on a treadmill, so the number that would normally answer "am I
 * getting fitter?" is permanently blank. This answers it from the same
 * evidence: hold effort constant by comparing only runs whose average heart
 * rate landed within ±8 bpm of each other, and look at what the pace did.
 *
 * The band is chosen by where the runs are, not by a round number — it is the
 * window containing the most comparable sessions, so the comparison is made
 * over the largest sample the data actually offers.
 */
export function fitnessAtFixedHr(activities: CachedActivity[]): Finding | null {
  const runs = scoredRuns(activities);
  if (runs.length < 6) return null;

  let band: CachedActivity[] = [];
  for (const a of runs) {
    const members = runs.filter((x) => Math.abs(x.avgHr! - a.avgHr!) <= HR_BAND);
    if (members.length > band.length) band = members;
  }
  if (band.length < 6) return null;

  const from = band[0].localDate!;
  const to = band[band.length - 1].localDate!;
  // Two months is the shortest window over which a change in aerobic fitness
  // is a change in aerobic fitness rather than a good day and a bad one.
  if (daysBetween(from, to) < 60) return null;

  const paces = band.map((a) => a.durationS! / 60 / (a.distanceM! / 1000));
  const k = Math.max(3, Math.floor(band.length / 3));
  const then = avg(paces.slice(0, k));
  const now = avg(paces.slice(-k));
  const gain = then - now; // positive means faster now
  if (Math.abs(gain) < 0.25) return null; // 15 s/km is inside the noise

  const hrs = band.map((a) => a.avgHr!);
  const lo = Math.min(...hrs);
  const hi = Math.max(...hrs);
  const faster = gain > 0;

  return {
    kind: "fitness-at-fixed-hr",
    section: "Fitness",
    tone: faster ? "good" : "watch",
    claim: faster
      ? `At the same heart rate you now run ${paceText(gain)} per kilometre faster.`
      : `At the same heart rate you're running ${paceText(-gain)} per kilometre slower than you were.`,
    detail: `Across ${band.length} runs whose average heart rate landed between ${lo} and ${hi} bpm — the same cost to you, whatever the treadmill said — the first ${k} averaged ${paceText(then)}/km and the last ${k} average ${paceText(now)}/km. This is the closest thing your data has to a fitness number: Garmin won't compute VO2 max without outdoor GPS runs, so it has never had one to give you. ${
      faster
        ? "Same heartbeats, more ground. That is what getting fitter is."
        : "Worth reading against the calendar — a block of harder, shorter sessions can do this without anything being wrong."
    }`,
    basis: `${band.length} runs at ${lo}–${hi} bpm · ${from} → ${to}`,
    series: [{ name: "Pace at fixed HR", values: paces, format: UNIT.pace, invert: true }],
    labels: band.map((a) => dayLabel(a.localDate!)),
  };
}

/**
 * What a step rate is actually worth, in seconds per kilometre.
 *
 * The app already nags about cadence. This says what the nag is worth, by
 * regressing metres-per-beat on cadence across every scored run and reading
 * the slope back out as pace at a representative heart rate. "Aim for 170" is
 * advice; "ten more steps a minute has been worth forty seconds a kilometre to
 * you" is a reason.
 */
export function cadenceLever(activities: CachedActivity[]): Finding | null {
  const runs = scoredRuns(activities).filter((a) => a.avgCadence != null);
  if (runs.length < 10) return null;

  const cadence = runs.map((a) => a.avgCadence!);
  const efficiency = runs.map((a) => metresPerBeat(a)!);
  const c = correlation(cadence, efficiency, 10);
  if (!c || c.r < 0.4) return null;

  const mx = avg(cadence);
  const my = avg(efficiency);
  const varX = cadence.reduce((t, x) => t + (x - mx) ** 2, 0);
  if (varX === 0) return null;
  const slope = cadence.reduce((t, x, i) => t + (x - mx) * (efficiency[i] - my), 0) / varX;
  const intercept = my - slope * mx;

  // Read the slope back out at a heart rate and a cadence you actually run at,
  // rather than at the mean of a series spanning a year of changing form.
  const recent = runs.slice(-5);
  const hr = avg(recent.map((a) => a.avgHr!));
  const cadNow = avg(recent.map((a) => a.avgCadence!));
  const paceAt = (spm: number) => 1000 / ((intercept + slope * spm) * hr);
  const gain = paceAt(cadNow) - paceAt(cadNow + 10);
  if (!isFinite(gain) || gain <= 0) return null;

  return {
    kind: "cadence-lever",
    section: "Fitness",
    tone: "note",
    claim: `Ten more steps a minute has been worth about ${Math.round(gain * 60)} seconds per kilometre to you.`,
    detail: `Cadence and metres-per-heartbeat move together across ${c.n} runs at r = ${c.r.toFixed(2)}. Some of that is one fact told twice, because on a treadmill turning the belt up raises both — so read the size of it rather than the direction. You average ${cadNow.toFixed(0)} spm against a usual target near 170, and at your recent ${hr.toFixed(0)} bpm the fitted line puts ${cadNow.toFixed(0)} → ${(cadNow + 10).toFixed(0)} spm at ${paceText(paceAt(cadNow))}/km → ${paceText(paceAt(cadNow + 10))}/km. Shorter, quicker contacts also cut joint load, which matters more the heavier you are.`,
    basis: `${c.n} runs · r = ${c.r.toFixed(2)} · slope ${(slope * 10).toFixed(3)} m/beat per 10 spm`,
    series: [
      { name: "Cadence", values: cadence, format: UNIT.spm },
      { name: "Per beat", values: efficiency, format: UNIT.perBeat, muted: true },
    ],
    labels: runs.map((a) => dayLabel(a.localDate!)),
  };
}

/* --------------------------------------------------------------- recovery --- */

interface Ledger {
  label: string;
  hrv: number;
  rhr: number | null;
  n: number;
}

/**
 * What each kind of day costs you overnight.
 *
 * Every training day is paired with the *following* morning's HRV and resting
 * heart rate, and both are read as a distance from your own baseline rather
 * than as absolute numbers — 74 ms means nothing without knowing that 74 ms is
 * your normal. Rest days are included as the control: without them a small
 * drop after a hard session is a number with nothing to be small against.
 */
export function recoveryLedger(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Finding | null {
  const rows = observed(daily);
  const index = new Map(rows.map((d) => [d.date, d]));
  const days = byDate(activities);

  const baseHrv = mean(rows.map((d) => d.hrvLastNight));
  const baseRhr = mean(rows.map((d) => d.restingHr));
  if (baseHrv == null) return null;

  const gather = (dates: string[]): Ledger | null => {
    const hrv: number[] = [];
    const rhr: number[] = [];
    for (const d of dates) {
      const next = index.get(shiftDate(d, 1));
      if (!next) continue;
      if (next.hrvLastNight != null) hrv.push(next.hrvLastNight - baseHrv);
      if (next.restingHr != null && baseRhr != null) rhr.push(next.restingHr - baseRhr);
    }
    if (hrv.length < 6) return null;
    return { label: "", hrv: avg(hrv), rhr: rhr.length ? avg(rhr) : null, n: hrv.length };
  };

  const trained = [...days.keys()].filter((d) => index.has(d));
  const hardDates = trained.filter(
    (d) => days.get(d)!.reduce((t, a) => t + hardSecs(a), 0) > HARD_DAY_SECS,
  );
  const easyDates = trained.filter((d) => !hardDates.includes(d));
  const restDates = rows.filter((d) => !days.has(d.date)).map((d) => d.date);

  const hard = gather(hardDates);
  const easy = gather(easyDates);
  const rest = gather(restDates);
  if (!hard || !rest) return null;

  const out: FindingRow[] = [];
  const push = (label: string, l: Ledger | null) => {
    if (!l) return;
    out.push({
      label,
      value: `${signed(l.hrv)} ms`,
      note: `${l.n} nights${l.rhr != null ? ` · RHR ${signed(l.rhr)}` : ""}`,
      accent: l.hrv <= -3,
    });
  };
  push("After a hard day", hard);
  push("After an easy day", easy);
  push("After a rest day", rest);

  // The same pairing by sport, for the sports with enough sessions to mean
  // anything. This is where the surprise usually is — the session that feels
  // hardest is often not the one your morning pays for.
  const sports = new Map<string, string[]>();
  for (const d of trained) {
    for (const a of days.get(d)!) {
      if (!a.typeKey) continue;
      const list = sports.get(a.typeKey);
      if (list) list.push(d);
      else sports.set(a.typeKey, [d]);
    }
  }
  const minutesOf = (key: string) => {
    const secs = activities
      .filter((a) => a.typeKey === key && a.durationS)
      .map((a) => a.durationS!);
    return secs.length ? avg(secs) / 60 : null;
  };
  const perSport = [...sports.entries()]
    .map(([key, dates]) => ({ key, l: gather([...new Set(dates)]), minutes: minutesOf(key) }))
    .filter(
      (s): s is { key: string; l: Ledger; minutes: number | null } => s.l != null && s.l.n >= 10,
    )
    .sort((a, b) => a.l.hrv - b.l.hrv);
  for (const s of perSport) push(sportLabel(s.key), s.l);

  // Against the easy day rather than the rest day. A rest day is a different
  // kind of day in more ways than the training — it is likelier to be a working
  // one — whereas easy and hard sessions differ mostly in the thing being asked
  // about, which makes it the fairer comparison to headline.
  const control = Math.max(easy?.hrv ?? rest.hrv, rest.hrv);
  const cost = control - hard.hrv;

  // The dearest sport is compared against the one you do most of, not against
  // the cheapest. The cheapest is usually something that isn't training at all
  // — a commute the watch decided to record — and beating that is not a fact
  // about your sessions.
  const dearest = perSport[0];
  const staple = perSport.reduce(
    (a, b) => (b.l.n > a.l.n ? b : a),
    perSport[0] ?? { key: "", l: hard, minutes: null },
  );

  // How long the dent lasts, read forward from the same hard days and against
  // the rest-day level rather than the raw baseline — the question is when a
  // morning after a hard day stops being distinguishable from a morning after
  // nothing, which is not the same as when it hits the mean of the year.
  const after = (k: number) => {
    const v = hardDates
      .map((d) => index.get(shiftDate(d, k))?.hrvLastNight)
      .filter((x): x is number => x != null);
    return v.length >= 6 ? avg(v) - baseHrv : null;
  };
  const trail = [2, 3].map((k) => ({ k, v: after(k) })).filter((t) => t.v != null);
  const cleared = trail.find((t) => Math.abs(t.v! - rest.hrv) <= 1)?.k;

  return {
    kind: "recovery-ledger",
    section: "Recovery",
    tone: cost >= 6 ? "watch" : "good",
    claim:
      cost >= 2
        ? `A hard session costs you about ${cost.toFixed(1)} ms of HRV overnight. An easy one costs you nothing.`
        : `Nothing you do to yourself in a session shows up in the next morning's HRV.`,
    detail: `Your baseline is ${baseHrv.toFixed(0)} ms across ${rows.filter((d) => d.hrvLastNight != null).length} nights. The morning after a day with more than five minutes above threshold it averages ${signed(hard.hrv)} ms against that; after a rest day, ${signed(rest.hrv)}${easy ? `; after an easy session, ${signed(easy.hrv)}` : ""}. ${
      cleared
        ? `By day ${cleared} it is back level with a rest-day morning, which is the number that matters — a hard Monday has not spent your Wednesday.`
        : trail.length
          ? `Two and three days out it still reads ${trail.map((t) => signed(t.v!)).join(" and ")}, so the return isn't clean.`
          : ""
    }${
      dearest && staple && dearest.key !== staple.key
        ? ` The split by sport below is the part worth reading twice: ${sportLabel(dearest.key).toLowerCase()} takes ${Math.abs(dearest.l.hrv - staple.l.hrv).toFixed(1)} ms more out of you overnight than ${sportLabel(staple.key).toLowerCase()} does${
            dearest.minutes != null && staple.minutes != null && dearest.minutes < staple.minutes
              ? `, at ${dearest.minutes.toFixed(0)} minutes a session against ${staple.minutes.toFixed(0)} — the shorter one is the dearer one`
              : ""
          }. A day carrying two sessions is counted under both.`
        : ""
    }`,
    basis: `${hard.n} mornings after hard days · ${rest.n} after rest days · baseline ${baseHrv.toFixed(0)} ms`,
    rows: out,
  };
}

/**
 * Whether consecutive training days accumulate.
 *
 * The received wisdom is that back-to-back sessions dig a hole. Whether they
 * dig yours is a question your own cache can answer: bucket every training day
 * by how many days deep into an unbroken run it was, and look at that morning's
 * HRV. If day four looks like day one, the hole is not being dug.
 */
export function streakTolerance(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Finding | null {
  const rows = observed(daily);
  const days = byDate(activities);
  const baseHrv = mean(rows.map((d) => d.hrvLastNight));
  if (baseHrv == null) return null;

  const buckets = new Map<number, number[]>();
  let streak = 0;
  for (const d of rows) {
    streak = days.has(d.date) ? streak + 1 : 0;
    if (!streak || d.hrvLastNight == null) continue;
    const k = Math.min(streak, 4);
    const list = buckets.get(k);
    if (list) list.push(d.hrvLastNight);
    else buckets.set(k, [d.hrvLastNight]);
  }

  const ordered = [1, 2, 3, 4]
    .map((k) => ({ k, v: buckets.get(k) ?? [] }))
    .filter((b) => b.v.length >= 8);
  if (ordered.length < 3) return null;

  const rowsOut: FindingRow[] = ordered.map((b) => ({
    label:
      b.k === 4 ? "Fourth day or deeper" : ["First", "Second", "Third"][b.k - 1] + " day in a row",
    value: `${avg(b.v).toFixed(0)} ms`,
    note: `${b.v.length} days · ${signed(avg(b.v) - baseHrv)} vs baseline`,
    accent: avg(b.v) - baseHrv <= -3,
  }));

  const first = avg(ordered[0].v);
  const deepest = avg(ordered[ordered.length - 1].v);
  const drop = first - deepest;
  const holds = drop < 3;

  return {
    kind: "streak-tolerance",
    section: "Recovery",
    tone: holds ? "good" : "watch",
    claim: holds
      ? "Training on consecutive days doesn't accumulate on you."
      : `By the ${ordered[ordered.length - 1].k === 4 ? "fourth" : "third"} day in a row your HRV is ${drop.toFixed(0)} ms down.`,
    detail: holds
      ? `Grouped by how deep into an unbroken run of training days each morning was, your HRV reads ${ordered.map((b) => `${avg(b.v).toFixed(0)}`).join(", ")} ms — flat, against a baseline of ${baseHrv.toFixed(0)}. That is a real constraint lifted: the thing limiting how much you train is not overnight recovery, so if you want more volume you can put it on consecutive days rather than hunting for gaps.`
      : `Grouped by depth into a run of training days, your HRV reads ${ordered.map((b) => `${avg(b.v).toFixed(0)}`).join(", ")} ms against a baseline of ${baseHrv.toFixed(0)}. The slide is the argument for a rest day before the third or fourth, not after it.`,
    basis: `${ordered.reduce((t, b) => t + b.v.length, 0)} mornings · baseline ${baseHrv.toFixed(0)} ms`,
    rows: rowsOut,
  };
}

/**
 * Whether Garmin's training readiness is worth obeying.
 *
 * Readiness is the number the watch puts in front of you every morning, and it
 * is the easiest thing in the app to organise a week around. So it is worth
 * knowing whether, for you specifically, it has ever predicted anything: score
 * each run by metres per heartbeat, pair it against that morning's readiness,
 * and see. Where the answer is "no", the finding is that the number is not the
 * instruction it looks like.
 */
export function readinessValue(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Finding | null {
  const index = new Map(observed(daily).map((d) => [d.date, d]));
  const runs = scoredRuns(activities).filter(
    (a) => index.get(a.localDate!)?.trainingReadiness != null,
  );
  if (runs.length < 10) return null;

  const ready = runs.map((a) => index.get(a.localDate!)!.trainingReadiness!);
  const quality = runs.map((a) => metresPerBeat(a)!);
  const c = correlation(ready, quality, 10);
  if (!c) return null;

  const best = runs
    .map((a, i) => ({ a, q: quality[i], r: ready[i] }))
    .sort((x, y) => y.q - x.q)
    .slice(0, 3);
  const weak = Math.abs(c.r) < 0.25;

  return {
    kind: "readiness-value",
    section: "Recovery",
    tone: "note",
    claim: weak
      ? "Your readiness score doesn't tell you how the run will go."
      : `Readiness does track how your runs go — r = ${c.r.toFixed(2)}.`,
    detail: weak
      ? `Over ${c.n} runs, that morning's readiness explains almost none of how much ground you covered per heartbeat (r = ${c.r.toFixed(2)}). Your best run by that measure came on a morning scored ${best[0].r.toFixed(0)} out of 100; the next two scored ${best
          .slice(1)
          .map((b) => b.r.toFixed(0))
          .join(
            " and ",
          )}. Readiness is largely a restatement of the night you just had, and this is the evidence that it is not also a forecast — a run skipped on a low score is a run skipped for no reason the data can find.`
      : `Over ${c.n} runs, mornings with a higher readiness score did go with covering more ground per heartbeat (r = ${c.r.toFixed(2)}). That is unusual enough to be worth using: on this account the score carries something about the session, not just about the night before it.`,
    basis: `${c.n} runs paired with that morning's score · r = ${c.r.toFixed(2)}`,
    series: [
      { name: "Readiness", values: ready, format: UNIT.score },
      { name: "Per beat", values: quality, format: UNIT.perBeat, muted: true },
    ],
    labels: runs.map((a) => dayLabel(a.localDate!)),
  };
}

/**
 * Every daily metric ranked by how much it actually moves your overnight HRV.
 *
 * Each column of the daily table is correlated against that night's HRV and
 * against the same morning's resting heart rate, and the results are sorted.
 * The ranking is the finding: it is usually not the metric anyone watches, and
 * training volume — the thing a beginner worries about most — routinely lands
 * at the bottom.
 *
 * Readiness and body battery are deliberately absent. Garmin computes both
 * partly *from* HRV, so they would top a table about what predicts HRV while
 * teaching nothing at all.
 */
export function recoveryDrivers(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Finding | null {
  const rows = observed(daily);
  if (rows.length < 60) return null;

  const load = new Map<string, number>();
  for (const a of activities) {
    if (!a.localDate) continue;
    load.set(a.localDate, (load.get(a.localDate) ?? 0) + edwardsTrimp(a));
  }

  const hrv = rows.map((d) => d.hrvLastNight);
  const rhr = rows.map((d) => d.restingHr);

  const candidates: Array<{ label: string; values: Point[] }> = [
    { label: "Sleep score", values: rows.map((d) => d.sleepScore) },
    {
      label: "Hours asleep",
      values: rows.map((d) => (d.sleepSecs == null ? null : d.sleepSecs / HOUR)),
    },
    { label: "Stress average", values: rows.map((d) => d.stressAvg) },
    { label: "Steps", values: rows.map((d) => d.steps) },
    { label: "Training load that day", values: rows.map((d) => load.get(d.date) ?? 0) },
    {
      label: "Training load the day before",
      values: rows.map((_, i) => (i ? (load.get(rows[i - 1].date) ?? 0) : null)),
    },
    {
      label: "Energy balance",
      values: rows.map((d) =>
        d.consumedKcal != null && d.totalBurnKcal != null ? d.consumedKcal - d.totalBurnKcal : null,
      ),
    },
  ];

  const ranked = candidates
    .map((c) => ({
      label: c.label,
      hrv: correlation(c.values, hrv, 25),
      rhr: correlation(c.values, rhr, 25),
    }))
    .filter(
      (
        c,
      ): c is {
        label: string;
        hrv: { r: number; n: number };
        rhr: { r: number; n: number } | null;
      } => c.hrv != null,
    )
    .sort((a, b) => Math.abs(b.hrv.r) - Math.abs(a.hrv.r));
  if (ranked.length < 4) return null;

  const top = ranked[0];
  const bottom = ranked[ranked.length - 1];
  const trainingRank = ranked.findIndex((c) => c.label.startsWith("Training load that day"));

  return {
    kind: "recovery-drivers",
    section: "Recovery",
    tone: "note",
    claim: `${top.label} moves your overnight HRV more than anything else you record.`,
    detail: `Every column of your daily table set against that night's HRV, strongest first. ${top.label} leads at r = ${top.hrv.r.toFixed(2)} over ${top.hrv.n} days; ${bottom.label} comes last at r = ${bottom.hrv.r.toFixed(2)}.${
      trainingRank >= 0
        ? ` How much you trained that day places ${ordinal(trainingRank + 1)} of the ${cardinal(ranked.length)} — worth holding onto if you have ever assumed a hard session is what your recovery numbers are reacting to.`
        : ""
    } Training readiness and body battery are missing on purpose: Garmin builds both partly out of HRV, so they would top a table about what predicts HRV while teaching you nothing. None of these is large enough to be a mechanism — the ranking is the thing to read, not any single figure.`,
    basis: `${ranked.length} metrics over ${rows.length} days · Pearson r against the same night's HRV`,
    rows: ranked.map((c) => ({
      label: c.label,
      value: c.hrv.r.toFixed(2),
      note: `${c.hrv.n} days${c.rhr ? ` · resting HR r = ${c.rhr.r.toFixed(2)}` : ""}`,
      accent: Math.abs(c.hrv.r) >= 0.25,
    })),
  };
}

/** Small counts read as words in a sentence; past eight, digits are fine. */
const ordinal = (n: number) =>
  ["first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth"][n - 1] ?? `${n}th`;

const cardinal = (n: number) =>
  ["one", "two", "three", "four", "five", "six", "seven", "eight"][n - 1] ?? `${n}`;

/* --------------------------------------------------------------- patterns --- */

const DAY_NAMES = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
const DAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

/**
 * Which day of the week your training actually falls on.
 *
 * A weekly plan is written as seven equivalent slots. A year of data says
 * otherwise: some weekday is always the one that gets skipped, and it is rarely
 * the one you'd name. Knowing which it is turns "train more" into "put the long
 * easy run on a Sunday", which is a thing a person can do.
 */
export function weekShape(daily: DailyMetrics[], activities: CachedActivity[]): Finding | null {
  const rows = observed(daily);
  if (rows.length < 60) return null;
  const days = byDate(activities);

  const stats = DAY_NAMES.map((_, i) => ({ i, seen: 0, trained: 0, sleep: [] as number[] }));
  for (const d of rows) {
    const date = parseLocal(d.date);
    if (!date) continue;
    const w = (date.getDay() + 6) % 7; // Monday = 0
    const s = stats[w];
    s.seen++;
    if (days.has(d.date)) s.trained++;
    if (d.sleepSecs != null) s.sleep.push(d.sleepSecs);
  }
  if (stats.some((s) => s.seen < 6)) return null;

  const rate = stats.map((s) => (s.trained / s.seen) * 100);
  const best = stats.reduce((a, b) => (rate[b.i] > rate[a.i] ? b : a));
  const worst = stats.reduce((a, b) => (rate[b.i] < rate[a.i] ? b : a));
  const second = stats
    .filter((s) => s.i !== worst.i)
    .reduce((a, b) => (rate[b.i] < rate[a.i] ? b : a));
  // A flat week is a fine outcome and not a finding.
  if (rate[best.i] - rate[worst.i] < 20) return null;

  return {
    kind: "week-shape",
    section: "Patterns",
    tone: "note",
    claim: `${DAY_NAMES[worst.i]} is the hole in your week.`,
    detail: `Across ${rows.length} days on the watch you trained on ${rate[worst.i].toFixed(0)}% of ${DAY_NAMES[worst.i]}s and ${rate[second.i].toFixed(0)}% of ${DAY_NAMES[second.i]}s, against ${rate[best.i].toFixed(0)}% of ${DAY_NAMES[best.i]}s. That is not a motivation problem to be fixed — it is the shape of your week, and it is more useful as a constraint than as a target. The one long easy run you owe yourself each week wants the day you're most likely to have an hour, not the day the plan says.`,
    basis: `${rows.length} days observed · ${[...days.keys()].length} of them with a session`,
    series: [{ name: "Trained", values: rate, format: UNIT.pct }],
    labels: DAY_SHORT,
    rows: stats.map((s) => ({
      label: DAY_NAMES[s.i],
      value: `${rate[s.i].toFixed(0)}%`,
      note: `${s.trained} of ${s.seen}${s.sleep.length ? ` · ${(avg(s.sleep) / HOUR).toFixed(1)} h asleep` : ""}`,
      accent: s.i === worst.i,
    })),
  };
}

/**
 * How a training day compares with a rest day on the metrics that aren't about
 * training at all.
 *
 * The intuition is that training is a cost paid in sleep and stress. Often the
 * data says the opposite, and it says it loudly enough to change how a rest day
 * gets planned — if your quiet days are the stressed ones, "rest" is doing
 * something other than resting.
 */
export function restDayContrast(
  daily: DailyMetrics[],
  activities: CachedActivity[],
): Finding | null {
  const rows = observed(daily);
  const days = byDate(activities);

  const split = (predicate: (d: DailyMetrics) => boolean) => {
    const set = rows.filter(predicate);
    return {
      n: set.length,
      stress: mean(set.map((d) => d.stressAvg)),
      sleepScore: mean(set.map((d) => d.sleepScore)),
      battery: mean(
        set.map((d) =>
          d.bodyBatteryHigh != null && d.bodyBatteryLow != null
            ? d.bodyBatteryHigh - d.bodyBatteryLow
            : null,
        ),
      ),
    };
  };

  const trained = split((d) => days.has(d.date));
  const rested = split((d) => !days.has(d.date));
  const hard = split(
    (d) => (days.get(d.date) ?? []).reduce((t, a) => t + hardSecs(a), 0) > HARD_DAY_SECS,
  );
  if (trained.n < 25 || rested.n < 25) return null;
  if (trained.stress == null || rested.stress == null) return null;
  if (trained.sleepScore == null || rested.sleepScore == null) return null;

  const stressGap = rested.stress - trained.stress;
  const sleepGap = trained.sleepScore - rested.sleepScore;
  // Both differences have to point the same way and be worth a sentence.
  if (stressGap < 1.5 || sleepGap < 1.5) return null;

  const rowsOut: FindingRow[] = [
    {
      label: "Training day",
      value: `${trained.stress.toFixed(0)} stress`,
      note: `${trained.n} days · sleep score ${trained.sleepScore.toFixed(0)}${trained.battery != null ? ` · ${trained.battery.toFixed(0)} pts of body battery spent` : ""}`,
    },
    {
      label: "Rest day",
      value: `${rested.stress.toFixed(0)} stress`,
      note: `${rested.n} days · sleep score ${rested.sleepScore.toFixed(0)}${rested.battery != null ? ` · ${rested.battery.toFixed(0)} pts spent` : ""}`,
      accent: true,
    },
  ];
  if (hard.n >= 15 && hard.stress != null && hard.sleepScore != null) {
    rowsOut.splice(1, 0, {
      label: "Hard training day",
      value: `${hard.stress.toFixed(0)} stress`,
      note: `${hard.n} days · sleep score ${hard.sleepScore.toFixed(0)}`,
    });
  }

  return {
    kind: "rest-day-contrast",
    section: "Patterns",
    tone: "note",
    claim: "Your rest days are the stressed ones, and you sleep worse on them.",
    detail: `On the ${trained.n} days you trained, your average stress score was ${trained.stress.toFixed(0)} and your sleep score ${trained.sleepScore.toFixed(0)}. On the ${rested.n} days you didn't, stress averaged ${rested.stress.toFixed(0)} and sleep scored ${rested.sleepScore.toFixed(0)}${hard.n >= 15 && hard.sleepScore != null ? ` — and the hard days scored best of all, at ${hard.sleepScore.toFixed(0)}` : ""}. The arrow could run either way: training may be settling you, or the days you skip may be the busy ones that were always going to score badly. Either reading argues against treating a rest day as free — it is doing something to you too.`,
    basis: `${trained.n} training days against ${rested.n} rest days`,
    rows: rowsOut,
  };
}

/**
 * The 80/20 line, month by month.
 *
 * The single thread this whole account is being coached on. Time above Z2 as a
 * share of run time, per calendar month, so drift is visible as a direction
 * rather than as one bad session — a run of hard weeks looks identical to a
 * hard week until you plot it.
 */
export function easyShareTrend(activities: CachedActivity[]): Finding | null {
  const runs = activities.filter((a) => isRun(a.typeKey) && a.localDate);
  const months = new Map<string, [number, number]>(); // [easy secs, total secs]
  for (const a of runs) {
    const key = a.localDate!.slice(0, 7);
    const total = a.zoneSecs.reduce((x, y) => x + y, 0);
    if (total <= 0) continue;
    const cur = months.get(key) ?? [0, 0];
    months.set(key, [cur[0] + a.zoneSecs[0] + a.zoneSecs[1], cur[1] + total]);
  }

  // Ten minutes of run time is the floor for a month to get a point on the
  // chart; below that one warm-up decides the month's percentage.
  const ordered = [...months.entries()]
    .filter(([, [, total]]) => total >= 600)
    .sort((a, b) => (a[0] < b[0] ? -1 : 1));
  if (ordered.length < 4) return null;

  const share = ordered.map(([, [easy, total]]) => (easy / total) * 100);
  const half = Math.floor(share.length / 2);
  const early = avg(share.slice(0, half));
  const late = avg(share.slice(-half));
  const now = share[share.length - 1];
  const rising = late > early;
  const latest = ordered[ordered.length - 1][0];
  // The window ends at today, so the last bucket is usually a month still in
  // progress. Saying "last month" about it would be wrong by a month.
  const current = latest === isoDate(new Date()).slice(0, 7);
  const named = current
    ? `your run time this month has been`
    : `${monthShort(latest)}'s run time was`;

  return {
    kind: "easy-share-trend",
    section: "Patterns",
    tone: now >= 60 ? "good" : rising ? "note" : "watch",
    claim: rising
      ? `Your easy share is climbing — ${now.toFixed(0)}% of ${named} in Z1–Z2.`
      : `Your easy share is falling — ${now.toFixed(0)}% of ${named} in Z1–Z2.`,
    detail: `Time in Z1–Z2 as a share of all run time, by month: ${ordered.map(([m], i) => `${monthShort(m)} ${share[i].toFixed(0)}%`).join(", ")}. The first half of that span averaged ${early.toFixed(0)}% and the second ${late.toFixed(0)}%. The 80/20 model wants 80 here, and ${now >= 60 ? "you are within reach of it" : `${now.toFixed(0)}% is a long way from it`} — which is a statement about where the hours go, not about whether the hard sessions are wrong. Two short hard runs plus one genuinely easy half-hour lands near 60% on its own.`,
    basis: `${ordered.length} months with at least 10 minutes of run time · ${ordered[0][0]} → ${latest}`,
    series: [{ name: "Easy share", values: share, format: UNIT.pct }],
    labels: ordered.map(([m]) => monthShort(m)),
  };
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

/* ----------------------------------------------------------- the whole set --- */

/**
 * Every finding the deeper analyses produce, in the order the screen renders
 * them. Each is independently nullable, so a thin cache simply yields a shorter
 * page rather than a page of caveats.
 */
export function deepFindings(daily: DailyMetrics[], activities: CachedActivity[]): Finding[] {
  return [
    fitnessAtFixedHr(activities),
    cadenceLever(activities),
    recoveryLedger(daily, activities),
    recoveryDrivers(daily, activities),
    streakTolerance(daily, activities),
    readinessValue(daily, activities),
    easyShareTrend(activities),
    weekShape(daily, activities),
    restDayContrast(daily, activities),
  ].filter((f): f is Finding => f != null);
}
