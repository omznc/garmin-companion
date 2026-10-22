// What to notify after a sync. This file is pure: no native module, no file, no clock.
// The wiring is in run.ts and notifications.ts.

export type NotifyPrefs = { enabled: boolean; activities: boolean; nudges: boolean };
export const DEFAULT_PREFS: NotifyPrefs = { enabled: false, activities: true, nudges: true };

// activities: the ids that the app has seen, newest first. null means no sync has run since
// install or since a sign-in, so the first run only records the ids and sends nothing.
// The memory of the nudges is in the core, which claims one notification for each kind and day.
export type NotifyState = { activities: number[] | null };
export const EMPTY_STATE: NotifyState = { activities: null };

export type Recent = { id: number; name: string | null };
// The notes that the Today screen shows. An accent note is one that is worth acting on.
// The kind is stable. The text holds live numbers, so it is not a key.
export type Attention = { accent: boolean; text: string; kind: string };

export type Planned = {
  channel: "activities" | "nudges";
  id: string;
  title: string;
  body: string;
  href: string;
  // Only a nudge has a kind.
  kind?: string;
};

export type Facts = {
  signedIn: boolean;
  sample: boolean;
  // True for a full history import: old activities are not news.
  history: boolean;
  // Local date of the Today view, `YYYY-MM-DD`.
  date: string;
  // Local hour of the day, 0 to 23.
  hour: number;
  recent: Recent[];
  attention: Attention[];
};

const SEEN_LIMIT = 200;
export const NUDGES_PER_RUN = 2;
// A nudge is not sent at night. The window is in local time.
const NUDGE_FROM_HOUR = 8;
const NUDGE_TO_HOUR = 21;

export function planActivities(recent: Recent[], seen: number[], prefs: NotifyPrefs): Planned[] {
  if (!prefs.enabled || !prefs.activities) return [];
  const known = new Set(seen);
  const fresh = recent.filter((r) => !known.has(r.id));
  const first = fresh[0];
  if (!first) return [];
  const names = fresh.map((r) => r.name?.trim()).filter(Boolean);
  const [title, body, href] =
    fresh.length === 1
      ? [
          "New activity synced",
          names[0] || "Open it to see the details.",
          `/today/activity/${first.id}`,
        ]
      : [
          `${fresh.length} new activities synced`,
          names.slice(0, 2).join(", ") || "Open the list to see them.",
          "/activities",
        ];
  return [{ channel: "activities", id: `activities-${first.id}`, title, body, href }];
}

// The nudges that may go out now. The core decides which ones went out already today.
export function planNudges(attention: Attention[], hour: number, prefs: NotifyPrefs): Planned[] {
  if (!prefs.enabled || !prefs.nudges) return [];
  if (hour < NUDGE_FROM_HOUR || hour >= NUDGE_TO_HOUR) return [];
  const kinds = new Set<string>();
  const out: Planned[] = [];
  for (const a of attention) {
    if (!a.accent || !a.text.trim() || !a.kind || kinds.has(a.kind)) continue;
    kinds.add(a.kind);
    out.push({
      channel: "nudges",
      id: `nudge-${a.kind}`,
      title: "Worth a look today",
      body: a.text,
      href: "/today",
      kind: a.kind,
    });
  }
  return out;
}

// Decide the notifications and the next state. Sample mode and a signed-out account change nothing.
// A quiet run (the first run, or a full history import) sends no notification of any kind.
export function planNotify(
  prefs: NotifyPrefs,
  state: NotifyState,
  facts: Facts,
): { activities: Planned[]; nudges: Planned[]; state: NotifyState } {
  if (facts.sample || !facts.signedIn) return { activities: [], nudges: [], state };
  const ids = facts.recent.map((r) => r.id);
  // A run that finds no ids must not erase the memory, so the memory only grows.
  const seen = state.activities ?? ids;
  const quiet = state.activities === null || facts.history;
  return {
    activities: quiet ? [] : planActivities(facts.recent, seen, prefs),
    nudges: quiet ? [] : planNudges(facts.attention, facts.hour, prefs),
    state: { activities: [...new Set([...ids, ...seen])].slice(0, SEEN_LIMIT) },
  };
}

// A tap carries a link in its data. Only the screens that the app sends are opened.
export function safeHref(value: unknown): string | null {
  if (typeof value !== "string") return null;
  return /^\/(today|activities|today\/activity\/\d+)$/.test(value) ? value : null;
}
