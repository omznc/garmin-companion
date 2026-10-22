// The `health_view` and `sleep_view` results of the Rust core, as JSON.
// The core owns the numbers and the sentences. The screens only draw them.
import { useQuery } from "@tanstack/react-query";
import { devPersona } from "./devPersona";
import { focusArgs, focusKey, useFocus } from "./focus";
import { clipHealth, clipSleep } from "./focusClip";
import { call } from "./source";

// The condition stays inline, so Metro drops the fixtures from a release build.
const fixtures = (__DEV__ ? require("./wellnessFixtures") : null) as
  | typeof import("./wellnessFixtures")
  | null;

// The wire types come from the Rust core, through `generated/`.
import type { HealthView } from "./generated/HealthView";
import type { MetricTrend } from "./generated/MetricTrend";
import type { SleepView } from "./generated/SleepView";

export type { HealthView, MetricTrend, SleepView };
export type { NightBar } from "./generated/NightBar";
export type { NightView } from "./generated/NightView";
export type { SleepInsight } from "./generated/SleepInsight";
export type { SleepWindow } from "./generated/SleepWindow";
export type { StageShare } from "./generated/StageShare";

const recorded = (cmd: string) =>
  devPersona ? fixtures?.wellnessFixture(cmd, devPersona) : undefined;

export const useHealth = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["health", focusKey(focus)],
    queryFn: async () =>
      (recorded("health_view") as HealthView | undefined) ?? call("health_view", focusArgs(focus)),
    // The core ends the view on the last day. A range also drops the days before its start.
    select: (view) => clipHealth(view, focus),
    retry: false,
  });
};

export const useSleep = () => {
  const focus = useFocus();
  return useQuery({
    queryKey: ["sleep", focusKey(focus)],
    queryFn: async () =>
      (recorded("sleep_view") as SleepView | undefined) ?? call("sleep_view", focusArgs(focus)),
    select: (view) => clipSleep(view, focus),
    retry: false,
  });
};

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

/** The first and last day of a metric window, as `11 Mar`. The last point is `view.date`. */
export function windowEnds(view: HealthView, m: MetricTrend): [string, string] {
  const last = new Date(`${view.date}T12:00:00`);
  const first = new Date(last);
  first.setDate(first.getDate() - (m.points.length - 1));
  const day = (d: Date) => d.toLocaleDateString("en-GB", { day: "2-digit", month: "short" });
  return [day(first), day(last)];
}
