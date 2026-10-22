// The pure parts of the date focus chip. Both platform chips use them.
import { type Focus, focusLabel, isLive, makeFocus } from "./focus.ts";

export type ChipState = { live: boolean; label: string };

/** A live focus shows the calendar button. Any other focus shows its label and a clear button. */
export const chipState = (focus: Focus, today: string): ChipState => ({
  live: isLive(focus, today),
  label: focusLabel(focus, today),
});

/** The body screens that read the date focus. The report, routes and gear do not. */
export const followsFocus = (kind: string): boolean =>
  ["weight", "food", "strength", "plan", "insights", "fitness"].includes(kind);

/** The `YYYY-MM-DD` of a date at UTC midnight. The Android picker gives its days in this form. */
export const utcDay = (date: Date): string => date.toISOString().slice(0, 10);

/** The focus of a picked range. No start gives no focus. No end gives a single day. */
export function pickedFocus(start: string | null, end: string | null, today: string): Focus {
  return start ? makeFocus(start, end ?? start, today) : null;
}
