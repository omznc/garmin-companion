// The Activities list and detail view models of the Rust core (`garmin_api::activities`).
// The core owns the figures and the sentences. The screens only draw them.
import { useQuery } from "@tanstack/react-query";
import { callJson } from "./source";

export type ActivityCard = {
  activityId: number;
  name: string;
  sportLabel: string;
  family: string;
  time: string | null;
  headline: string;
  detail: string;
};

export type ActivitiesView = {
  empty: boolean;
  groups: { date: string; label: string; sessions: ActivityCard[] }[];
};

export type ActivityDetail = {
  activityId: number;
  name: string;
  sportLabel: string;
  family: string;
  when: string;
  stats: { label: string; value: string }[];
  zones: { rows: { zone: number; percent: number; time: string }[]; sentence: string } | null;
  laps: {
    label: string;
    distance: string | null;
    time: string | null;
    pace: string | null;
    avgHr: string | null;
  }[];
  strength: {
    sentence: string;
    sets: {
      label: string;
      reps: string | null;
      work: string | null;
      rest: string | null;
      exercise: string | null;
    }[];
  } | null;
  chart: { hr: (number | null)[]; elevation: (number | null)[]; span: string } | null;
};

// Dev only: EXPO_PUBLIC_PERSONA shows a recorded persona, as it does for Today.
const personas = (__DEV__ ? require("./activityPersonas") : null) as
  | typeof import("./activityPersonas")
  | null;
const persona = __DEV__
  ? personas?.activityPersonas[process.env.EXPO_PUBLIC_PERSONA ?? ""]
  : undefined;

export const fetchActivities = () =>
  persona
    ? Promise.resolve(persona.list as ActivitiesView)
    : callJson<ActivitiesView>("activities", { limit: 60 });

// A persona has one recorded detail. Another id has none.
export const fetchActivityDetail = (id: number) => {
  const recorded = persona?.detail as ActivityDetail | null | undefined;
  return persona
    ? Promise.resolve(recorded?.activityId === id ? recorded : null)
    : callJson<ActivityDetail | null>("activity_detail", { activityId: id });
};

export const useActivities = () =>
  useQuery({ queryKey: ["activities"], queryFn: fetchActivities, retry: false });

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
