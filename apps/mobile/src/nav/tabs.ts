// The five tabs, in the shipped order. Settings live in More.
// The core lists the movable tabs too: `MOVABLE_TABS` in crates/garmin-api/src/settings.rs. Keep both in step.
export const TABS = [
  { name: "today", title: "Today", sf: "sun.max", sfSelected: "sun.max.fill" },
  { name: "activities", title: "Activities", sf: "figure.run", sfSelected: "figure.run" },
  { name: "health", title: "Health", sf: "heart", sfSelected: "heart.fill" },
  { name: "sleep", title: "Sleep", sf: "moon.zzz", sfSelected: "moon.zzz.fill" },
  { name: "more", title: "More", sf: "ellipsis.circle", sfSelected: "ellipsis.circle.fill" },
] as const;

export type Tab = (typeof TABS)[number];
export type TabName = Tab["name"];

/** More stays last, and the person cannot move it. */
export const LAST_TAB: TabName = "more";

const byName = (name: string) => TABS.find((t) => t.name === name);

/** The tabs that the person can move, in the shipped order. */
export const MOVABLE: readonly Tab[] = TABS.filter((t) => t.name !== LAST_TAB);

/** The saved order as the movable tabs. An unknown or repeated key is dropped, a missing tab follows. */
export function movableTabs(saved: readonly string[]): Tab[] {
  const seen = new Set<string>();
  const kept = saved.flatMap((name) => {
    const tab = MOVABLE.find((t) => t.name === name);
    if (!tab || seen.has(name)) return [];
    seen.add(name);
    return [tab];
  });
  return [...kept, ...MOVABLE.filter((t) => !seen.has(t.name))];
}

/** Every tab in the bar order: the movable tabs as saved, then More. */
export function orderedTabs(saved: readonly string[]): Tab[] {
  return [...movableTabs(saved), ...TABS.filter((t) => t.name === LAST_TAB)];
}

/** The tab that the app opens on. An unknown value gives Today. */
export function startTab(saved: string): Tab {
  return byName(saved) ?? TABS[0];
}

/** The order after one tab moves by one place. A move past an end changes nothing. */
export function moveTab(saved: readonly string[], name: string, by: -1 | 1): TabName[] {
  const names = movableTabs(saved).map((t) => t.name);
  const from = names.indexOf(name as TabName);
  const to = from + by;
  const mine = names[from];
  const other = names[to];
  if (!mine || !other) return names;
  names[from] = other;
  names[to] = mine;
  return names;
}

/** True when the order and the start tab are the shipped ones. */
export function isShipped(saved: readonly string[], start: string): boolean {
  const same = movableTabs(saved).every((t, i) => t.name === MOVABLE[i]?.name);
  return same && startTab(start).name === TABS[0].name;
}

export type TabPrefs = { order: string[]; start: string };

/** The tab order and the start tab as the app saves them in tabs.json. */
export const SHIPPED_PREFS: TabPrefs = { order: [], start: "" };

/** Read the text of tabs.json. A missing or broken file gives the shipped values. */
export function parseTabPrefs(text: string): TabPrefs {
  try {
    const p = JSON.parse(text) as Partial<TabPrefs> | null;
    const order = Array.isArray(p?.order) ? p.order.filter((k) => typeof k === "string") : [];
    return { order, start: typeof p?.start === "string" ? p.start : "" };
  } catch {
    return SHIPPED_PREFS;
  }
}

/** The core is the source of truth. The prefs to apply when the core differs from the copy, else null. */
export function coreWins(
  core: { tab_order: readonly string[]; start_tab: string },
  copy: TabPrefs,
): TabPrefs | null {
  const same = core.start_tab === copy.start && core.tab_order.join() === copy.order.join();
  return same ? null : { order: [...core.tab_order], start: core.start_tab };
}

/** The screens in the router state: the start tab first, then the rest in the saved order. */
export function launchScreens(prefs: TabPrefs): Tab[] {
  const first = startTab(prefs.start);
  return [first, ...orderedTabs(prefs.order).filter((t) => t.name !== first.name)];
}

/**
 * Run `send` for one value at a time. A value that arrives during a send waits, and a newer
 * value replaces it, so the last value is the one that is stored.
 */
export function latestWins<T>(send: (value: T) => Promise<void>): (value: T) => Promise<void> {
  let waiting: { value: T } | null = null;
  let running = false;
  return async (value) => {
    waiting = { value };
    if (running) return;
    running = true;
    try {
      while (waiting) {
        const { value: next } = waiting;
        waiting = null;
        await send(next);
      }
    } finally {
      running = false;
    }
  };
}
