/** Formatting helpers. Every one returns "—" rather than "null" or "NaN". */

export const DASH = "—";

export function km(metres: number | null | undefined, digits = 1): string {
  if (metres == null || metres <= 0) return DASH;
  return `${(metres / 1000).toFixed(digits)} km`;
}

/** "1:02:11" over an hour, "48:22" under. */
export function duration(secs: number | null | undefined): string {
  if (secs == null || secs <= 0) return DASH;
  const s = Math.round(secs);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(sec)}` : `${m}:${pad(sec)}`;
}

/** "7h 12m", for sleep and weekly totals. */
export function hoursMinutes(secs: number | null | undefined): string {
  if (secs == null || secs <= 0) return DASH;
  const total = Math.round(secs / 60);
  return `${Math.floor(total / 60)}h ${String(total % 60).padStart(2, "0")}m`;
}

/** Minutes-per-km, the unit every run in this account is read in. */
export function pace(
  metres: number | null | undefined,
  secs: number | null | undefined,
): string {
  if (!metres || !secs || metres < 50) return DASH;
  const minPerKm = secs / 60 / (metres / 1000);
  if (!isFinite(minPerKm) || minPerKm > 60) return DASH;
  const m = Math.floor(minPerKm);
  const s = Math.round((minPerKm - m) * 60);
  return s === 60 ? `${m + 1}:00` : `${m}:${String(s).padStart(2, "0")}`;
}

/** Cycling and other sports read better as speed than as pace. */
export function speed(
  metres: number | null | undefined,
  secs: number | null | undefined,
): string {
  if (!metres || !secs) return DASH;
  return `${(metres / 1000 / (secs / 3600)).toFixed(1)} km/h`;
}

export function num(
  v: number | null | undefined,
  digits = 0,
  suffix = "",
): string {
  if (v == null || Number.isNaN(v)) return DASH;
  return v.toLocaleString("en-GB", {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }) + suffix;
}

export function bpm(v: number | null | undefined): string {
  return v == null ? DASH : `${Math.round(v)}`;
}

/** `treadmill_running` → `Treadmill running`. */
export function sportLabel(typeKey: string | null | undefined): string {
  if (!typeKey) return "Activity";
  const s = typeKey.replace(/_v\d+$/, "").replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

export const isRun = (typeKey: string | null | undefined) =>
  !!typeKey && typeKey.includes("running");

/* -------------------------------------------------------------- calendar --- */

const MONTHS = [
  "January", "February", "March", "April", "May", "June",
  "July", "August", "September", "October", "November", "December",
];
const DAYS = [
  "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
];

/** Parses `YYYY-MM-DD` or `YYYY-MM-DD HH:MM:SS` as *local* time.
 *  `new Date("2026-08-07")` would parse as UTC and shift the date backwards
 *  for anyone west of Greenwich. */
export function parseLocal(s: string | null | undefined): Date | null {
  if (!s) return null;
  const m = s.match(
    /^(\d{4})-(\d{2})-(\d{2})(?:[ T](\d{2}):(\d{2})(?::(\d{2}))?)?/,
  );
  if (!m) return null;
  return new Date(
    +m[1], +m[2] - 1, +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0),
  );
}

/** "Friday, 7 August" */
export function longDate(d: Date): string {
  return `${DAYS[d.getDay()]}, ${d.getDate()} ${MONTHS[d.getMonth()]}`;
}

/** "06 Aug" — the fixed-width form the activity list aligns on. */
export function shortDate(d: Date): string {
  return `${String(d.getDate()).padStart(2, "0")} ${MONTHS[d.getMonth()].slice(0, 3)}`;
}

export function monthLabel(d: Date): string {
  return `${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
}

export function timeOfDay(d: Date): string {
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

/** Local `YYYY-MM-DD`, matching how `local_date` is stored. */
export function isoDate(d: Date): string {
  return [
    d.getFullYear(),
    String(d.getMonth() + 1).padStart(2, "0"),
    String(d.getDate()).padStart(2, "0"),
  ].join("-");
}

export function daysAgo(n: number): string {
  const d = new Date();
  d.setDate(d.getDate() - n);
  return isoDate(d);
}

/** "4 minutes ago" — how long ago an instant was, in the coarsest unit that
 *  still says something. An exact timestamp is precision nobody reads: what
 *  you want to know about a sync is whether it was recent. Falls back to a
 *  date once "days ago" stops being a useful way to say it.
 *
 *  Takes a full timestamp with an offset (as `last_sync` is stored), not the
 *  bare local dates `parseLocal` handles. */
export function since(iso: string | null | undefined, now = new Date()): string {
  if (!iso) return DASH;
  const then = new Date(iso);
  if (Number.isNaN(then.getTime())) return DASH;

  const secs = Math.round((now.getTime() - then.getTime()) / 1000);
  // A clock skew or a sync that lands mid-render shouldn't read "in -2 seconds".
  if (secs < 45) return "just now";

  const mins = Math.round(secs / 60);
  if (mins < 60) return plural(mins, "minute");
  const hours = Math.round(mins / 60);
  if (hours < 24) return plural(hours, "hour");

  // Calendar days, not 24-hour blocks: a sync at 23:00 last night is
  // "yesterday" at 08:00, however few hours have actually passed.
  const days = Math.round(
    (startOfDay(now).getTime() - startOfDay(then).getTime()) / 86_400_000,
  );
  if (days <= 1) return "yesterday";
  if (days < 14) return `${days} days ago`;
  return `on ${shortDate(then)}${then.getFullYear() === now.getFullYear() ? "" : ` ${then.getFullYear()}`}`;
}

const plural = (n: number, unit: string) => `${n} ${unit}${n === 1 ? "" : "s"} ago`;

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate());

