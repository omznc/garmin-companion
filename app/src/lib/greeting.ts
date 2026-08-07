/**
 * The line at the top of Today.
 *
 * It used to be three strings — morning, afternoon, evening — which is fine
 * once and wallpaper by the third day. This gives every part of the day its
 * own set, including the hours either side of midnight that "Good evening"
 * had nothing to say about, and lets some lines use your name.
 *
 * Two rules the lines have to follow:
 *
 * - **The punctuation is part of the line.** "You're still up, huh?" must not
 *   get a full stop bolted on, so nothing downstream appends one.
 * - **`{name}` is a slot, not a guarantee.** Lines containing it are only
 *   eligible when a name is actually known, so a fresh install never greets
 *   you as "Good evening, .".
 */

interface Bucket {
  /** Hour the bucket starts, local. Runs until the next one begins. */
  from: number;
  /** Whether "It's Friday" fits here. It does not at two in the morning. */
  weekdayFlavour?: boolean;
  lines: string[];
}

/**
 * Ordered by start hour, covering the full 24. The tone tracks the clock: dry
 * and slightly conspiratorial after midnight, plain in the working day.
 */
const BUCKETS: Bucket[] = [
  {
    from: 0,
    lines: [
      "You're still up, huh?",
      "Still up, {name}?",
      "It's past midnight.",
      "The small hours, {name}.",
      "Nothing good gets decided at this hour.",
      "Late one.",
      "This counts as tomorrow, technically.",
      "Burning the candle, {name}?",
      "Everyone else is asleep.",
    ],
  },
  {
    from: 4,
    lines: [
      "Up before the sun.",
      "Early start, {name}.",
      "It's barely morning.",
      "First light.",
      "You're up early, {name}.",
      "The quiet part of the day.",
    ],
  },
  {
    from: 6,
    weekdayFlavour: true,
    lines: [
      "Good morning.",
      "Good morning, {name}.",
      "Morning, {name}.",
      "Morning.",
      "A new day's numbers.",
      "Fresh set of numbers, {name}.",
      "Here's where things stand.",
    ],
  },
  {
    from: 11,
    weekdayFlavour: true,
    lines: [
      "Good afternoon.",
      "Afternoon, {name}.",
      "Middle of the day.",
      "Halfway through, {name}.",
      "Good afternoon, {name}.",
      "Midday check-in.",
    ],
  },
  {
    from: 14,
    weekdayFlavour: true,
    lines: [
      "Good afternoon.",
      "Good afternoon, {name}.",
      "Afternoon.",
      "Afternoon, {name}.",
      "Still plenty of day left.",
      "How's it going, {name}?",
    ],
  },
  {
    from: 18,
    weekdayFlavour: true,
    lines: [
      "Good evening.",
      "Good evening, {name}.",
      "Evening, {name}.",
      "Evening.",
      "Day's mostly done.",
      "Let's see how today went.",
      "Evening, {name} — here's the day.",
    ],
  },
  {
    from: 22,
    lines: [
      "Getting late.",
      "Winding down, {name}?",
      "Late evening.",
      "Nearly tomorrow.",
      "Wrapping up, {name}?",
      "Last look before bed?",
    ],
  },
];

/**
 * Lines that only make sense on one weekday. They compete with the usual set
 * rather than replacing it — a Monday should still mostly get a plain
 * "Good morning." — and only in the buckets marked `weekdayFlavour`.
 */
const BY_WEEKDAY: Record<number, string[]> = {
  0: ["Sunday.", "Sunday, {name}.", "Long-run weather?"],
  1: ["Monday again.", "New week, {name}.", "Week one, day one."],
  5: ["Friday.", "Made it to Friday, {name}.", "It's Friday."],
  6: ["Saturday.", "Saturday, {name}.", "Weekend, {name}."],
};

/**
 * How often a weekday line wins, as one-in-N. Fixed rather than folded into
 * the bucket's pool: the pools are small, so appending three Friday lines to a
 * pool of seven made Friday say "It's Friday" four times in a day.
 */
const WEEKDAY_ODDS = 5;

export interface GreetingOptions {
  /** Usually a first name. Lines with a `{name}` slot are skipped without it. */
  name?: string | null;
  now?: Date;
}

export function greeting({ name, now = new Date() }: GreetingOptions = {}): string {
  const hour = now.getHours();
  const bucket =
    [...BUCKETS].reverse().find((b) => hour >= b.from) ?? BUCKETS[0];

  // Seeded on the date and the hour rather than Math.random(): the line has to
  // hold still while you look at it, and React will re-render this several
  // times a minute. It moves on when the hour does.
  const seed = hash(
    `${now.getFullYear()}-${now.getMonth()}-${now.getDate()}-${hour}`,
  );

  const weekday = bucket.weekdayFlavour ? (BY_WEEKDAY[now.getDay()] ?? []) : [];
  // A second, independent draw off the same seed decides which set to read
  // from, so the odds don't shift with how many lines a bucket happens to have.
  const preferWeekday = weekday.length > 0 && seed % WEEKDAY_ODDS === 0;

  const usable = (lines: string[]) =>
    lines.filter((line) => name || !line.includes("{name}"));

  const pool = preferWeekday ? usable(weekday) : usable(bucket.lines);
  const fallback = usable(preferWeekday ? bucket.lines : weekday);

  const chosen = pool.length ? pool : fallback;
  if (!chosen.length) return "Hello.";

  const line = chosen[(seed >>> 8) % chosen.length];
  return name ? line.replace("{name}", name) : line;
}

/** Trims a Garmin profile name down to the part you'd actually be called. */
export function firstName(full: string | null | undefined): string | null {
  const first = (full ?? "").trim().split(/\s+/)[0];
  // Guard against a display name that's really a handle — "omznc1994" reads
  // worse than no name at all.
  return first && /^\p{L}[\p{L}'’-]*$/u.test(first) ? first : null;
}

/** FNV-1a, purely so the same day and hour always land on the same line. */
function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}
