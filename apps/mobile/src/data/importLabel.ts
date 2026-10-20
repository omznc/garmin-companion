// The words of the import row on Today. This file has no React, so a test can read it.
import type { SyncState } from "./useSync";

const DAYS_PER_MONTH = 30;

// Days in the quick pass of the import. The label and the sync call share it.
export const QUICK_DAYS = 14;

export type ImportLabelInput = Pick<
  SyncState,
  "importing" | "importPass" | "phase" | "done" | "total" | "line"
>;

// The pull-to-refresh spinner stays off during the import, which has its own row.
export const isPulling = (s: Pick<SyncState, "busy" | "importing">) => s.busy && !s.importing;

// Until the first data arrives, the import replaces the lede of Today.
export function importLede<T extends { lede: string }>(
  importing: boolean,
  hasHero: boolean,
  data: T,
): T {
  return importing && !hasHero ? { ...data, lede: "Getting your Garmin data." } : data;
}

// Null means: show no row. The row hides when the import ends, not when done equals total.
export function importLabel(s: ImportLabelInput): string | null {
  if (!s.importing) return null;
  if (s.importPass !== "full") return `Getting your last ${QUICK_DAYS} days`;
  if (s.phase === "wellness" && s.total) {
    const months = Math.max(1, Math.round(s.total / DAYS_PER_MONTH));
    const done = Math.min(months, Math.floor(s.done / DAYS_PER_MONTH));
    return `Your history · ${done} of ${months} ${months === 1 ? "month" : "months"}`;
  }
  return s.line ?? "Getting your history";
}
