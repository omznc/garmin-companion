// The date focus: one day or a from-to range that every screen and the coach read.
// It matches `garmin_api::focus` (the desktop calendar focus). `null` is the live day:
// it follows the real today and sends no date to the core.
import { useSyncExternalStore } from "react";

/** `YYYY-MM-DD` dates. A single day has `from === to`. */
export type Focus = { from: string; to: string } | null;

const DAY_MS = 86_400_000;

// A date at local noon, so a daylight-saving change cannot move the day.
function atNoon(iso: string): Date {
  return new Date(`${iso}T12:00:00`);
}

/** The local `YYYY-MM-DD` of a date. */
export function isoDay(date: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${p(date.getMonth() + 1)}-${p(date.getDate())}`;
}

/** A focus from two dates in any order. A date after `today` is not allowed: it becomes `today`. */
export function makeFocus(a: string, b: string, today: string): NonNullable<Focus> {
  const [x, y] = [a, b].map((d) => (d > today ? today : d)).sort();
  return { from: x as string, to: y as string };
}

/** The number of days with both ends. A single day is 1. */
export function focusDays(f: NonNullable<Focus>): number {
  return Math.round((atNoon(f.to).getTime() - atNoon(f.from).getTime()) / DAY_MS) + 1;
}

/** True for the live focus, and for the single day of the real today. */
export function isLive(f: Focus, today: string): boolean {
  return f === null || (f.from === f.to && f.to === today);
}

/** The arguments that the core commands take. The live focus sends none. */
export function focusArgs(f: Focus): { from: string; to: string } | undefined {
  return f ? { from: f.from, to: f.to } : undefined;
}

/** A query key part: a new focus makes a new query. */
export function focusKey(f: Focus): string {
  return f ? `${f.from}..${f.to}` : "live";
}

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

// Fixed names, because the locale spells September as "Sept".
const day = (iso: string, withYear: boolean) => {
  const d = atNoon(iso);
  return `${d.getDate()} ${MONTHS[d.getMonth()]}${withYear ? ` ${d.getFullYear()}` : ""}`;
};

/** The text of the chip: "Today", "Wed 14 Oct" or "14 - 20 Oct 2026". Same rules as the desktop. */
export function focusLabel(f: Focus, today: string): string {
  if (isLive(f, today) || f === null) return "Today";
  if (f.from === f.to) return `${WEEKDAYS[atNoon(f.to).getDay()]} ${day(f.to, false)}`;
  const [s, e] = [atNoon(f.from), atNoon(f.to)];
  if (s.getFullYear() !== e.getFullYear()) return `${day(f.from, true)} - ${day(f.to, true)}`;
  if (s.getMonth() !== e.getMonth()) return `${day(f.from, false)} - ${day(f.to, true)}`;
  return `${s.getDate()} - ${day(f.to, true)}`;
}

/** The ready ranges of the picker. Each one ends on `today`. */
export function focusPresets(today: string): { label: string; focus: NonNullable<Focus> }[] {
  const back = (n: number) => isoDay(new Date(atNoon(today).getTime() - n * DAY_MS));
  const first = `${today.slice(0, 8)}01`;
  return [
    { label: "Yesterday", focus: { from: back(1), to: back(1) } },
    { label: "Last 7 days", focus: { from: back(6), to: today } },
    { label: "Last 30 days", focus: { from: back(29), to: today } },
    { label: "This month", focus: { from: first, to: today } },
  ];
}

// One focus for the whole app. A screen reads it with `useFocus`.
let current: Focus = null;
const listeners = new Set<() => void>();

export const getFocus = (): Focus => current;

export function setFocus(next: Focus) {
  if (focusKey(next) === focusKey(current)) return;
  current = next;
  for (const l of listeners) l();
}

export const clearFocus = () => setFocus(null);

const subscribeFocus = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};

export const useFocus = (): Focus => useSyncExternalStore(subscribeFocus, getFocus, getFocus);

/** The real today on this device. */
export const todayIso = () => isoDay(new Date());
