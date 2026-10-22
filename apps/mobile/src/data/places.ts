// Plan, Routes and Gear. The core builds the numbers and the sentences
// (`garmin_api::plan`, `routes` and `gear`). This file types the JSON and turns
// it into the blocks that `screens/Body.*` draws.
import type { Block, BodyScreenData } from "./body";
import { devPersona } from "./devPersona";
import { call } from "./source";

// The condition stays inline, so Metro drops the fixtures from a release build.
const fixtures = (__DEV__ ? require("./placesFixtures") : null) as
  | typeof import("./placesFixtures")
  | null;

// The wire types come from the Rust core, through `generated/`.
import type { GearView } from "./generated/GearView";
import type { PlanView } from "./generated/PlanView";
import type { RoutesView } from "./generated/RoutesView";

export type { GearView, PlanView, RoutesView };

export type PlacesKind = "plan" | "routes" | "gear";

const DASH = "—";
const join = (parts: (string | null | undefined)[]) => parts.filter(Boolean).join(" · ");
const note = (text: string): Block => ({ kind: "text", text, tone: "note" });

export function presentPlan(v: PlanView): BodyScreenData {
  const title = "Plan";
  if (v.empty) return { title, blocks: [{ kind: "empty", ...v.empty }, note(v.gapNote)] };
  const blocks: Block[] = [];
  if (v.sentence) blocks.push({ kind: "text", text: v.sentence, tone: "lede" });
  blocks.push({ kind: "tiles", tiles: v.tiles });
  blocks.push({
    kind: "rows",
    title: "Saved workouts",
    rows: v.workouts.map((w) => ({
      key: w.name,
      value: w.size ?? DASH,
      sub: join([w.sport, w.description]),
    })),
  });
  blocks.push(note(v.gapNote));
  return { title, blocks };
}

export function presentRoutes(v: RoutesView): BodyScreenData {
  const title = "Routes";
  if (v.empty) return { title, blocks: [{ kind: "empty", ...v.empty }] };
  return {
    title,
    blocks: [
      { kind: "tiles", tiles: v.tiles },
      ...v.notes.map(note),
      {
        kind: "rows",
        title: "Latest first",
        rows: v.routes.map((r) => ({
          key: r.name,
          value: r.distance ?? DASH,
          sub: join([r.sport, r.timesLine, r.outings[0] && `last ${r.outings[0].date}`]),
          trace: r.trace,
        })),
      },
    ],
  };
}

export function presentGear(v: GearView): BodyScreenData {
  const title = "Gear";
  if (v.empty) return { title, blocks: [{ kind: "empty", ...v.empty }] };
  return {
    title,
    blocks: [
      {
        kind: "rows",
        title: "Registered gear",
        rows: v.items.map((g) => ({
          key: g.name,
          value: g.distance ?? DASH,
          sub: join([g.kind, g.status, g.detail]),
          bar: g.wearPct == null ? undefined : { fraction: g.wearPct / 100, accent: g.accent },
        })),
      },
      note(v.note),
    ],
  };
}

// Dev only: EXPO_PUBLIC_PERSONA shows a made-up user, as it does for Today.
const recorded = (cmd: string) =>
  devPersona ? fixtures?.placesFixture(cmd, devPersona) : undefined;

export async function fetchPlaces(kind: PlacesKind): Promise<BodyScreenData> {
  if (kind === "plan") {
    return presentPlan(
      (recorded("plan_view") as PlanView | undefined) ?? (await call("plan_view")),
    );
  }
  if (kind === "routes") {
    const view = (recorded("routes_view") as RoutesView | undefined) ?? (await call("routes_view"));
    return presentRoutes(view);
  }
  return presentGear((recorded("gear_view") as GearView | undefined) ?? (await call("gear_view")));
}
