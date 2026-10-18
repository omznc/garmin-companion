// Weight, Food and Strength. The core builds the numbers and the sentences
// (`garmin_api::body`). This file types the JSON and turns it into blocks.
// The screens draw the blocks and have no other copy.
import { useQuery } from "@tanstack/react-query";
import { callJson } from "./source";

export type EmptyNote = { title: string; body: string };

export type WeightView = {
  empty: EmptyNote | null;
  count: number;
  sentence: string | null;
  trendKg: number | null;
  latestKg: number | null;
  changeKg: number | null;
  bmi: number | null;
  windows: { days: number; sentence: string }[];
  points: { date: string; kg: number; trendKg: number | null; outlier: boolean }[];
  composition: {
    date: string;
    bodyFatPct: number | null;
    bodyWaterPct: number | null;
    muscleKg: number | null;
    boneKg: number | null;
  } | null;
  goalSentence: string | null;
};

export type FoodView = {
  empty: EmptyNote | null;
  daysLogged: number;
  avgEatenKcal: number | null;
  avgBurnedKcal: number | null;
  avgBalanceKcal: number | null;
  goalKcal: number | null;
  sentence: string;
  water: { avgMl: number; goalMl: number | null; sentence: string } | null;
  days: {
    date: string;
    label: string;
    logged: boolean;
    eatenKcal: number | null;
    burnedKcal: number | null;
    balanceKcal: number | null;
    waterMl: number | null;
  }[];
};

export type StrengthView = {
  shown: boolean;
  empty: EmptyNote | null;
  sessionsExamined: number;
  avgSets: number | null;
  avgReps: number | null;
  medianRestS: number | null;
  identifiedPct: number | null;
  sentence: string;
  caveat: string;
  sessions: { activityId: number; label: string; summary: string; movements: string | null }[];
  exercises: {
    name: string;
    sessions: number;
    sets: number;
    reps: number;
    history: { reps: number }[];
  }[];
};

export type BodyKind = "weight" | "food" | "strength";

export type Block =
  | { kind: "text"; text: string; tone?: "lede" | "note" | "heading" }
  | { kind: "tiles"; tiles: { label: string; value: string; accent?: boolean }[] }
  | { kind: "chart"; title: string; values: number[]; from: string; to: string }
  | { kind: "rows"; title: string; rows: { key: string; value: string; sub?: string }[] }
  | { kind: "empty"; title: string; body: string };

export type BodyScreenData = { title: string; blocks: Block[] };

const DASH = "—";
const whole = (v: number | null) => (v == null ? DASH : Math.round(v).toLocaleString("en-US"));
const one = (v: number | null) => (v == null ? DASH : v.toFixed(1));
const signed = (v: number | null) =>
  v == null ? DASH : `${v > 0 ? "+" : ""}${Math.round(v).toLocaleString("en-US")}`;
const mmss = (s: number) => `${Math.floor(s / 60)}:${String(Math.round(s % 60)).padStart(2, "0")}`;

function tilesOf(items: [string, string, boolean?][]): Block {
  // A tile with no figure is left out.
  const tiles = items
    .filter(([, value]) => value !== DASH)
    .map(([label, value, accent]) => ({ label, value, accent }));
  return { kind: "tiles", tiles };
}

function present(blocks: (Block | null | false)[]): Block[] {
  return blocks.filter((b): b is Block => !!b);
}

export function presentWeight(v: WeightView): BodyScreenData {
  const title = "Weight";
  if (v.empty) return { title, blocks: [{ kind: "empty", ...v.empty }] };
  const first = v.points[0];
  const last = v.points[v.points.length - 1];
  const c = v.composition;
  const comp = c
    ? tilesOf([
        ["Body fat", c.bodyFatPct == null ? DASH : `${one(c.bodyFatPct)} %`],
        ["Body water", c.bodyWaterPct == null ? DASH : `${one(c.bodyWaterPct)} %`],
        ["Muscle", c.muscleKg == null ? DASH : `${one(c.muscleKg)} kg`],
        ["Bone", c.boneKg == null ? DASH : `${one(c.boneKg)} kg`],
      ])
    : null;
  return {
    title,
    blocks: present([
      v.sentence ? { kind: "text", text: v.sentence, tone: "lede" } : null,
      tilesOf([
        ["Trend", v.trendKg == null ? DASH : `${one(v.trendKg)} kg`],
        ["Latest", v.latestKg == null ? DASH : `${one(v.latestKg)} kg`],
        ["Change", v.changeKg == null ? DASH : `${v.changeKg > 0 ? "+" : ""}${one(v.changeKg)} kg`],
        ["BMI", one(v.bmi)],
      ]),
      ...v.windows.map((w): Block => ({ kind: "text", text: w.sentence, tone: "note" })),
      first && last
        ? {
            kind: "chart",
            title: "Trend",
            values: v.points.map((p) => p.trendKg ?? p.kg),
            from: first.date,
            to: last.date,
          }
        : null,
      comp && { kind: "text", text: "Body composition", tone: "heading" },
      comp,
      v.goalSentence ? { kind: "text", text: v.goalSentence } : null,
      {
        kind: "rows",
        title: "By day",
        rows: [...v.points]
          .reverse()
          .slice(0, 60)
          .map((p) => ({
            key: p.date,
            value: `${one(p.kg)} kg`,
            sub: p.outlier ? "Check this reading" : undefined,
          })),
      },
    ]),
  };
}

export function presentFood(v: FoodView): BodyScreenData {
  const logged = v.days.filter((d) => d.logged).reverse();
  const first = logged[0];
  const last = logged[logged.length - 1];
  const eaten = logged.map((d) => d.eatenKcal ?? 0);
  return {
    title: "Food",
    blocks: present([
      v.empty
        ? { kind: "empty", ...v.empty }
        : tilesOf([
            ["Avg eaten", whole(v.avgEatenKcal)],
            ["Avg burned", whole(v.avgBurnedKcal)],
            ["Avg balance", signed(v.avgBalanceKcal), (v.avgBalanceKcal ?? 0) > 0],
          ]),
      !v.empty && { kind: "text", text: v.sentence, tone: "note" },
      // An unset goal draws no goal line, and the sentence names the goal only when it exists.
      first && last && eaten.length > 1
        ? { kind: "chart", title: "Eaten", values: eaten, from: first.label, to: last.label }
        : null,
      v.water ? { kind: "text", text: "Water", tone: "heading" } : null,
      v.water ? { kind: "text", text: v.water.sentence } : null,
      {
        kind: "rows",
        title: "By day",
        rows: v.days.map((d) => ({
          key: d.label,
          value: d.logged ? `${whole(d.eatenKcal)} kcal` : "no food logged",
          sub: [
            d.logged ? `burned ${whole(d.burnedKcal)}, balance ${signed(d.balanceKcal)}` : null,
            d.waterMl ? `${(d.waterMl / 1000).toFixed(2)} L water` : null,
          ]
            .filter(Boolean)
            .join(", "),
        })),
      },
    ]),
  };
}

export function presentStrength(v: StrengthView): BodyScreenData {
  const title = "Strength";
  if (v.empty) return { title, blocks: [{ kind: "empty", ...v.empty }] };
  return {
    title,
    blocks: present([
      { kind: "text", text: v.sentence, tone: "lede" },
      tilesOf([
        ["Sets per session", whole(v.avgSets)],
        ["Reps per session", whole(v.avgReps)],
        ["Typical rest", v.medianRestS == null ? DASH : mmss(v.medianRestS)],
        ["Sets identified", v.identifiedPct == null ? DASH : `${v.identifiedPct} %`],
      ]),
      { kind: "text", text: v.caveat, tone: "note" },
      v.exercises.length > 0
        ? {
            kind: "rows",
            title: "By movement",
            rows: v.exercises.map((e) => ({
              key: e.name,
              value: `${e.reps} reps`,
              sub: `${e.sessions} sessions, ${e.sets} sets`,
            })),
          }
        : null,
      {
        kind: "rows",
        title: "Sessions",
        rows: v.sessions.map((s) => ({
          key: s.label,
          value: s.summary,
          sub: s.movements ?? undefined,
        })),
      },
    ]),
  };
}

const commands = { weight: "weight_view", food: "food_view", strength: "strength_view" } as const;

async function fetchBody(kind: BodyKind): Promise<BodyScreenData> {
  if (kind === "weight") return presentWeight(await callJson<WeightView>(commands.weight));
  if (kind === "food") return presentFood(await callJson<FoodView>(commands.food));
  return presentStrength(await callJson<StrengthView>(commands.strength));
}

export const useBody = (kind: BodyKind) =>
  useQuery({ queryKey: ["body", kind], queryFn: () => fetchBody(kind), retry: false });

// The Health list shows Strength only when the profile shows gym sessions.
export const useStrengthShown = () =>
  useQuery({
    queryKey: ["body", "strength-shown"],
    queryFn: async () => (await callJson<StrengthView>(commands.strength)).shown,
    retry: false,
  });
