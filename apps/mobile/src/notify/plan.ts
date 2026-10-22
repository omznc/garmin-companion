// What to notify after a sync. This file is pure: no native module, no file, no clock.
// The wiring is in run.ts and notifications.ts.

export type NotifyPrefs = { enabled: boolean; activities: boolean; nudges: boolean };
export const DEFAULT_PREFS: NotifyPrefs = { enabled: false, activities: true, nudges: true };

// activities: the ids that the app has seen, newest first. null means no sync has run since
// install, so the first run only records the ids and sends nothing.
// nudges: one "date|key" entry for each nudge that went out today.
export type NotifyState = { activities: number[] | null; nudges: string[] };
export const EMPTY_STATE: NotifyState = { activities: null, nudges: [] };

export type Recent = { id: number; name: string | null };
// The notes that the Today screen shows. An accent note is one that is worth acting on.
export type Attention = { accent: boolean; text: string; insight?: string | null };

export type Planned = {
  channel: "activities" | "nudges";
  id: string;
  title: string;
  body: string;
  href: string;
};

export type Facts = {
  signedIn: boolean;
  sample: boolean;
  // True for a full history import: old activities are not news.
  history: boolean;
  // Local date of the Today view, `YYYY-MM-DD`.
  date: string;
  recent: Recent[];
  attention: Attention[];
};

const SEEN_LIMIT = 200;
const NUDGES_PER_RUN = 2;

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
      : [`${fresh.length} new activities synced`, names.slice(0, 2).join(", "), "/activities"];
  return [{ channel: "activities", id: `activities-${first.id}`, title, body, href }];
}

export function planNudges(
  attention: Attention[],
  date: string,
  sent: string[],
  prefs: NotifyPrefs,
): Planned[] {
  if (!prefs.enabled || !prefs.nudges) return [];
  const today = sent.filter((s) => s.startsWith(`${date}|`));
  return attention
    .filter((a) => a.accent && a.text.trim() && !today.includes(nudgeKey(date, a)))
    .slice(0, NUDGES_PER_RUN)
    .map((a) => ({
      channel: "nudges",
      id: `nudge-${nudgeKey(date, a)}`,
      title: "Worth a look today",
      body: a.text,
      href: "/today",
    }));
}

const nudgeKey = (date: string, a: Attention) => `${date}|${a.insight ?? a.text}`;

// Decide the notifications and the next state. Sample mode and a signed-out account change nothing.
export function planNotify(
  prefs: NotifyPrefs,
  state: NotifyState,
  facts: Facts,
): { planned: Planned[]; state: NotifyState } {
  if (facts.sample || !facts.signedIn) return { planned: [], state };
  const ids = facts.recent.map((r) => r.id);
  // A run that finds no ids must not erase the memory, so the memory only grows.
  const seen = state.activities ?? ids;
  const quiet = state.activities === null || facts.history;
  const planned = quiet ? [] : planActivities(facts.recent, seen, prefs);
  const nudges = planNudges(facts.attention, facts.date, state.nudges, prefs);
  planned.push(...nudges);
  return {
    planned,
    state: {
      activities: [...new Set([...ids, ...seen])].slice(0, SEEN_LIMIT),
      nudges: [
        ...state.nudges.filter((s) => s.startsWith(`${facts.date}|`)),
        ...nudges.map((n) => n.id.replace(/^nudge-/, "")),
      ],
    },
  };
}

// A tap carries a link in its data. Only the screens that the app sends are opened.
export function safeHref(value: unknown): string | null {
  if (typeof value !== "string") return null;
  return /^\/(today|activities|today\/activity\/\d+)$/.test(value) ? value : null;
}
