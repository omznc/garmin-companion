// The words of the import row on Today. This file has no React, so a test can read it.
import type { SyncState } from "./useSync";

const DAYS_PER_MONTH = 30;

export type ImportLabelInput = Pick<
  SyncState,
  "importing" | "full" | "phase" | "done" | "total" | "line"
>;

// Null means: show no row. The row hides when the import ends, not when done equals total.
export function importLabel(s: ImportLabelInput): string | null {
  if (!s.importing) return null;
  if (!s.full) return "Getting your last 14 days";
  if (s.phase === "wellness" && s.total) {
    const months = Math.max(1, Math.round(s.total / DAYS_PER_MONTH));
    const done = Math.min(months, Math.floor(s.done / DAYS_PER_MONTH));
    return `Your history · ${done} of ${months} months`;
  }
  return s.line ?? "Getting your history";
}
