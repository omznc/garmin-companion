// The tab order and the start tab. The Rust core keeps them (`set_tab_order`). A copy
// lives in a small file that is read before the first screen draws, because the bridge
// is async and the tab bar must not flash the shipped order. The same plan as theme/prefs.ts.
import { File, Paths } from "expo-file-system";
import { useSyncExternalStore } from "react";
import { call } from "../data/source";
import { notifyError } from "../ui/notify-error";

export type TabPrefs = { order: string[]; start: string };

const SHIPPED: TabPrefs = { order: [], start: "" };
const file = () => new File(Paths.document, "tabs.json");

const read = (): TabPrefs => {
  try {
    const p = JSON.parse(file().textSync()) as Partial<TabPrefs>;
    const order = Array.isArray(p.order) ? p.order.filter((k) => typeof k === "string") : [];
    return { order, start: typeof p.start === "string" ? p.start : "" };
  } catch {
    // No file yet, or an unreadable one: the shipped order is a complete tab bar.
    return SHIPPED;
  }
};

let current = read();
const listeners = new Set<() => void>();

/** The copy at launch. The iOS tab bar reads it once, because a native bar cannot reorder. */
export const launchTabPrefs: TabPrefs = current;

const apply = (next: TabPrefs) => {
  current = next;
  for (const l of listeners) l();
  try {
    const f = file();
    if (!f.exists) f.create();
    f.write(JSON.stringify(next));
  } catch {
    // The choice still applies until the app closes. The core keeps it.
  }
};

export const getTabPrefs = () => current;

export const useTabPrefs = (): TabPrefs =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => current,
  );

/** Apply the choice at once, then save it in the core. A failed save shows the reason. */
export async function saveTabPrefs(next: TabPrefs) {
  apply(next);
  try {
    await call("set_tab_order", { tabOrder: next.order, startTab: next.start });
  } catch (e) {
    notifyError("Could not save the tabs", e);
  }
}

/** The core is the source of truth. Take its value when the copy differs, for example after a restore. */
export async function syncTabPrefs() {
  const core = await call("prefs").catch(() => null);
  if (!core) return;
  const same = core.start_tab === current.start && core.tab_order.join() === current.order.join();
  if (!same) apply({ order: core.tab_order, start: core.start_tab });
}
