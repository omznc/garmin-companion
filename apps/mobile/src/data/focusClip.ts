// Clip the series of a view to the start of a focus range. The core ends a view on the
// last day. The start is clipped here, like `Focus::dated` in `garmin_api::focus`.
import type { Focus } from "./focus.ts";
import type { HealthView } from "./generated/HealthView";
import type { SleepView } from "./generated/SleepView";

const DAY_MS = 86_400_000;

// `YYYY-MM-DD` of `date` minus `back` days. Local noon keeps a daylight-saving change out.
function minusDays(date: string, back: number): string {
  const d = new Date(new Date(`${date}T12:00:00`).getTime() - back * DAY_MS);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
}

/** A single day and the live focus keep every point. A range drops the days before `from`. */
export function clipHealth(view: HealthView, focus: Focus): HealthView {
  if (!focus || focus.from === focus.to) return view;
  const metrics = view.metrics.map((m) => {
    const n = m.points.length;
    return {
      ...m,
      points: m.points.filter((_, i) => minusDays(view.date, n - 1 - i) >= focus.from),
    };
  });
  return { ...view, metrics, days: Math.max(0, ...metrics.map((m) => m.points.length)) };
}

/** A range keeps the nights inside it. */
export function clipSleep(view: SleepView, focus: Focus): SleepView {
  if (!focus || focus.from === focus.to) return view;
  const inside = (b: { date: string }) => b.date >= focus.from && b.date <= focus.to;
  const window = (w: SleepView["week"]) => ({ ...w, bars: w.bars.filter(inside) });
  return { ...view, week: window(view.week), month: window(view.month) };
}
