// The `health_view` and `sleep_view` results of the Rust core, as JSON.
// The core owns the numbers and the sentences. The screens only draw them.
import { useQuery } from "@tanstack/react-query";
import { callJson } from "./source";
import { devPersona, wellnessFixture } from "./wellnessFixtures";

export type MetricTrend = {
  key: string;
  label: string;
  // Empty for a unitless score.
  unit: string;
  valueText: string;
  caption: string;
  // One slot for each day, oldest first. A day with no reading is null.
  points: (number | null)[];
  sentence: string;
};

export type HealthView = {
  date: string;
  days: number;
  empty: boolean;
  note: string;
  metrics: MetricTrend[];
};

export type StageShare = {
  key: "deep" | "light" | "rem" | "awake";
  label: string;
  secs: number;
  pct: number;
  text: string;
};

export type NightView = {
  date: string;
  title: string;
  score: number | null;
  qualifier: string | null;
  durationSecs: number;
  durationText: string;
  start: string | null;
  end: string | null;
  stages: StageShare[];
  sentence: string;
};

export type NightBar = { date: string; label: string; hours: number | null; score: number | null };

export type SleepWindow = { days: number; bars: NightBar[]; summary: string };

export type SleepInsight = {
  id: string;
  tone: "good" | "note" | "watch";
  claim: string;
  detail: string;
  nights: number;
};

export type SleepView = {
  date: string;
  empty: boolean;
  lede: string;
  note: string | null;
  lastNight: NightView | null;
  week: SleepWindow;
  month: SleepWindow;
  insights: SleepInsight[];
};

async function view<T>(cmd: string): Promise<T> {
  const recorded = devPersona ? wellnessFixture(cmd, devPersona) : undefined;
  return (recorded as T | undefined) ?? callJson<T>(cmd);
}

export const useHealth = () =>
  useQuery({ queryKey: ["health"], queryFn: () => view<HealthView>("health_view"), retry: false });

export const useSleep = () =>
  useQuery({ queryKey: ["sleep"], queryFn: () => view<SleepView>("sleep_view"), retry: false });

/** Scale a series to 0.15..1 for bars, so a small change still shows. Gaps stay null. */
export function scaled(points: (number | null)[]): (number | null)[] {
  const real = points.filter((p): p is number => p !== null);
  const lo = Math.min(...real);
  const span = Math.max(...real) - lo || 1;
  return points.map((p) => (p === null ? null : 0.15 + ((p - lo) / span) * 0.85));
}

/** `2026-03-11` as `11 Mar`. */
export function shortDay(date: string): string {
  return new Date(`${date}T12:00:00`).toLocaleDateString("en-GB", {
    day: "2-digit",
    month: "short",
  });
}
