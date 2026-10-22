// Weight, Food and Strength. The core builds the numbers and the sentences
// (`garmin_api::body`). This file types the JSON and turns it into blocks.
// The screens draw the blocks and have no other copy.
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { bodyLine, DASH, one, whole } from "./bodyLine";
import { fetchPlaces, type PlacesKind } from "./places";
import { call } from "./source";
import {
  type FitnessView,
  type InsightsView,
  presentFitness,
  presentInsights,
  presentReport,
  type ReportView,
} from "./training";

// The wire types come from the Rust core, through `generated/`.
import type { EmptyNote } from "./generated/EmptyNote";
import type { FoodView } from "./generated/FoodView";
import type { StrengthView } from "./generated/StrengthView";
import type { WeightView } from "./generated/WeightView";

export type { EmptyNote, FoodView, StrengthView, WeightView };

export type BodyKind = "weight" | "food" | "strength";
/** A screen that draws blocks. Insights, Fitness and the weekly report come from `training.ts`. */
export type ScreenKind = BodyKind | "insights" | "fitness" | "report" | PlacesKind;

/** A row may carry the shape of a route, or a wear bar from 0 to 1. */
export type BlockRow = {
  key: string;
  value: string;
  sub?: string;
  trace?: [number, number][];
  bar?: { fraction: number; accent: boolean };
};

export type Block =
  | { kind: "text"; text: string; tone?: "lede" | "note" | "heading" }
  | { kind: "tiles"; tiles: { label: string; value: string; accent?: boolean }[] }
  | { kind: "chart"; title: string; values: number[]; from: string; to: string }
  | { kind: "rows"; title: string; rows: BlockRow[] }
  | { kind: "empty"; title: string; body: string }
  | { kind: "actions"; actions: { id: string; label: string }[] };

export type BodyScreenData = { title: string; blocks: Block[] };

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
            from: first.label,
            to: last.label,
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
            key: p.label,
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
              value: e.repsText,
              sub: e.detail,
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

async function fetchBody(kind: ScreenKind, offset: number): Promise<BodyScreenData> {
  if (kind === "insights") return presentInsights(await call("insights_view"));
  if (kind === "fitness") return presentFitness(await call("fitness_view"));
  if (kind === "report") return presentReport(await call("report_view", { offset }));
  if (kind === "plan" || kind === "routes" || kind === "gear") return fetchPlaces(kind);
  if (kind === "weight") return presentWeight(await call(commands.weight));
  if (kind === "food") return presentFood(await call(commands.food));
  return presentStrength(await call(commands.strength));
}

export const useBody = (kind: ScreenKind, offset = 0) =>
  useQuery({
    queryKey: ["body", kind, offset],
    queryFn: () => fetchBody(kind, offset),
    // Keep the old week on screen while the next one loads.
    placeholderData: keepPreviousData,
    // Gear asks Garmin live, so it must not refetch on each focus.
    staleTime: kind === "gear" ? 10 * 60_000 : 0,
    retry: false,
  });

// Fitness shows only when the cache has a training status or a record.
export const useFitnessShown = () =>
  useQuery({
    queryKey: ["body", "fitness-shown"],
    queryFn: async () => (await call("fitness_view")).empty == null,
    retry: false,
  });

// The Health list shows Strength only when the profile shows gym sessions.
export const useStrengthShown = () =>
  useQuery({
    queryKey: ["body", "strength-shown"],
    queryFn: async () => (await call(commands.strength)).shown,
    retry: false,
  });

/** The value line of a row on the Health and More lists. */
export const useBodyLine = (kind: BodyKind) =>
  useQuery({
    queryKey: ["body", "line", kind],
    queryFn: async () => bodyLine(kind, await call(commands[kind])),
    retry: false,
  });
