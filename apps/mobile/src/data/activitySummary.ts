// The plain text of one session, for the share sheet.
import type { ActivityDetailView } from "./generated/ActivityDetailView";

export function activitySummary(d: ActivityDetailView): string {
  const head = [d.name, [d.sportLabel, d.when].filter(Boolean).join(", ")].filter(Boolean);
  return [...head, ...d.stats.map((s) => `${s.label}: ${s.value}`)].join("\n");
}
