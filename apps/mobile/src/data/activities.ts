// The Activities list and detail view models of the Rust core (`garmin_api::activities`).
// The core owns the figures and the sentences. The screens only draw them.
import { useQuery } from "@tanstack/react-query";
import { type Focus, focusArgs, focusKey, useFocus } from "./focus";
import { call } from "./source";

// The wire types come from the Rust core, through `generated/`.
import type { ActivitiesView } from "./generated/ActivitiesView";
import type { ActivityDetailView } from "./generated/ActivityDetailView";

export type { ActivitiesView };
export type { ActivityCard } from "./generated/ActivityCard";
export type ActivityDetail = ActivityDetailView;

// Dev only: EXPO_PUBLIC_PERSONA shows a recorded persona, as it does for Today.
const personas = (__DEV__ ? require("./activityPersonas") : null) as
  | typeof import("./activityPersonas")
  | null;
const persona = personas?.activityPersonas[process.env.EXPO_PUBLIC_PERSONA ?? ""];

export const fetchActivities = (focus: Focus = null) =>
  persona
    ? Promise.resolve(persona.list as ActivitiesView)
    : call("activities", { limit: 60, ...focusArgs(focus) });

// A persona has one recorded detail. Another id has none.
export const fetchActivityDetail = (id: number) => {
  const recorded = persona?.detail as ActivityDetail | null | undefined;
  return persona
    ? Promise.resolve(recorded?.activityId === id ? recorded : null)
    : call("activity_detail", { activityId: id });
};

export const useActivities = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["activities", focusKey(focus)],
    queryFn: () => fetchActivities(focus),
    retry: false,
  });
};

export const useActivityDetail = (id: number) =>
  useQuery({
    queryKey: ["activity", id],
    queryFn: () => fetchActivityDetail(id),
    retry: false,
  });

// Heights from 0 to 1 for a bar chart. A gap is 0. The lowest sample gets a short bar.
export function barHeights(values: (number | null)[]): number[] {
  const real = values.filter((v): v is number => v !== null);
  if (real.length === 0) return [];
  const lo = Math.min(...real);
  const span = Math.max(...real) - lo || 1;
  return values.map((v) => (v === null ? 0 : 0.12 + 0.88 * ((v - lo) / span)));
}
