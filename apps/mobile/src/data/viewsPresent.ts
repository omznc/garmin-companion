// The pure presenters for the tiles, extras and charts views. They have no react import,
// so the node tests can load them. A section with no data gives nothing.
import type { Block } from "./body";
import type { BedWake } from "./generated/BedWake";
import type { ChartsView } from "./generated/ChartsView";
import type { ExtrasView } from "./generated/ExtrasView";
import type { Meal } from "./generated/Meal";
import type { TilesView } from "./generated/TilesView";

const DAY_MS = 86_400_000;
const whole = (n: number) => Math.round(n).toLocaleString("en-US");
const pad = (n: number) => String(n).padStart(2, "0");
const join = (parts: (string | null | undefined)[]) => parts.filter(Boolean).join(" · ");

/** The text of a clock minute, from minutes after midnight: "23:05". */
export function clockText(minute: number): string {
  const m = Math.round(minute) % 1440;
  return `${pad(Math.floor(m / 60))}:${pad(m % 60)}`;
}

function hoursMinutes(secs: number): string {
  const m = Math.round(secs / 60);
  return `${Math.floor(m / 60)} h ${pad(m % 60)} min`;
}

/** "14 nights, 7 h 15 min short". Nothing for a debt under one minute. */
export function sleepDebtLine(v: TilesView): string | null {
  const d = v.sleepDebt;
  if (!d || d.debtSecs < 60) return null;
  return `${d.nights} nights, ${hoursMinutes(d.debtSecs)} short`;
}

/** Bed time, wake time and the bed time spread, as label and value pairs. */
export function bedWakeFacts(b: BedWake | null): [string, string][] {
  if (!b) return [];
  return [
    ["Bedtime", clockText(b.bedMinute)],
    ["Wake time", clockText(b.wakeMinute)],
    ["Consistency", `±${Math.round(b.consistencyMins)} min`],
  ];
}

export type SleepExtras = {
  debt: string | null;
  facts: [string, string][];
  /** The 14 nights, oldest first. A night with no score is null. Null when no night has one. */
  scores: { average: string | null; points: (number | null)[]; label: string } | null;
};

export function presentSleepExtras(v: TilesView): SleepExtras | null {
  const debt = sleepDebtLine(v);
  const facts = bedWakeFacts(v.bedWake);
  const { points, average } = v.sleepScores;
  const real = points.filter((p): p is number => p !== null);
  const scores =
    real.length === 0
      ? null
      : {
          average: average == null ? null : String(Math.round(average)),
          points,
          label: `Sleep score, last ${points.length} nights: ${real.map((p) => Math.round(p)).join(", ")}`,
        };
  return debt || facts.length > 0 || scores ? { debt, facts, scores } : null;
}

/** The whole days from `today` to `date`, both `YYYY-MM-DD`. */
export function daysTo(date: string, today: string): number {
  const at = (s: string) => new Date(`${s}T12:00:00Z`).getTime();
  return Math.round((at(date) - at(today)) / DAY_MS);
}

/** The target time as "3:45:00". */
export function clockSecs(secs: number): string {
  const s = Math.round(secs);
  return `${Math.floor(s / 3600)}:${pad(Math.floor((s % 3600) / 60))}:${pad(s % 60)}`;
}

function daysText(n: number): string {
  if (n < 0) return "past";
  if (n === 0) return "today";
  return n === 1 ? "1 day to go" : `${n} days to go`;
}

function raceBlocks(v: ExtrasView): Block[] {
  const r = v.race;
  if (!r) return [];
  const tiles = [
    { label: "Race", value: r.name ?? "Race" },
    { label: "Date", value: r.date },
    { label: "Countdown", value: daysText(daysTo(r.date, v.date)), accent: true },
    ...(r.targetSecs == null ? [] : [{ label: "Target", value: clockSecs(r.targetSecs) }]),
    ...(r.location ? [{ label: "Location", value: r.location }] : []),
  ];
  return [
    { kind: "text", text: "Next race", tone: "heading" },
    { kind: "tiles", tiles },
  ];
}

function plannedBlocks(v: ExtrasView): Block[] {
  if (v.planned.length === 0) return [];
  return [
    {
      kind: "rows",
      title: "Next 14 days",
      rows: v.planned.map((p) => {
        const n = p.steps?.steps.length ?? 0;
        return {
          key: p.title ?? p.steps?.name ?? (p.isRace ? "Race" : "Workout"),
          value: p.date,
          sub: join([p.sport, n > 0 ? `${n} steps` : null]) || undefined,
        };
      }),
    },
  ];
}

const mealRow = (m: Meal) => ({
  key: m.name ?? "Meal",
  value: m.kcal == null ? "no calories" : `${whole(m.kcal)} kcal`,
  sub: join([m.time, macros(m.proteinG, m.carbsG, m.fatG)]) || undefined,
});

function macros(p: number | null, c: number | null, f: number | null): string {
  const part = (label: string, g: number | null) => (g == null ? null : `${label} ${whole(g)} g`);
  return join([part("protein", p), part("carbs", c), part("fat", f)]);
}

function nutritionTiles(n: ExtrasView["nutrition"]) {
  const g = n.goals;
  const goal = g?.adjustedCalories ?? g?.calories ?? null;
  const macroGoal = g ? macros(g.proteinG, g.carbsG, g.fatG) : "";
  return [
    ...(n.loggedKcal == null ? [] : [{ label: "Logged", value: `${whole(n.loggedKcal)} kcal` }]),
    ...(goal == null ? [] : [{ label: "Goal", value: `${whole(goal)} kcal` }]),
    ...(macroGoal ? [{ label: "Macro goal", value: macroGoal }] : []),
  ];
}

function nutritionBlocks(v: ExtrasView): Block[] {
  const n = v.nutrition;
  if (n.meals.length === 0 && n.loggedKcal == null) return [];
  const tiles = nutritionTiles(n);
  const blocks: Block[] = [{ kind: "text", text: "Nutrition today", tone: "heading" }];
  if (tiles.length > 0) blocks.push({ kind: "tiles", tiles });
  if (n.meals.length > 0) blocks.push({ kind: "rows", title: "Meals", rows: n.meals.map(mealRow) });
  return blocks;
}

/** The blocks that the Plan screen gets from the extras view. Empty when there is nothing. */
export function presentPlanExtras(v: ExtrasView): Block[] {
  return [...raceBlocks(v), ...plannedBlocks(v), ...nutritionBlocks(v)];
}

/** The muscle group bars and the weekly sets of the Strength screen. */
export function presentStrengthCharts(v: ChartsView): Block[] {
  const { groups, unassignedSets } = v.setsByMuscle;
  const blocks: Block[] = [];
  if (groups.length > 0) {
    const top = Math.max(...groups.map((g) => g.sets), 1);
    blocks.push({
      kind: "rows",
      title: "Sets by muscle group",
      rows: [
        ...groups.map((g, i) => ({
          key: g.group,
          value: `${g.sets} sets`,
          bar: { fraction: g.sets / top, accent: i === 0 },
        })),
        ...(unassignedSets > 0 ? [{ key: "Not assigned", value: `${unassignedSets} sets` }] : []),
      ],
    });
  }
  const weeks = v.setsPerWeek;
  const first = weeks[0];
  const last = weeks.at(-1);
  if (first && last && weeks.length > 1) {
    blocks.push({
      kind: "chart",
      title: "Sets per week",
      values: weeks.map((w) => w.sets),
      from: first.isoWeek,
      to: last.isoWeek,
    });
  }
  return blocks;
}
