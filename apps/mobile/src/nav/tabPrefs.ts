// The tab order and the start tab. The Rust core keeps them (`set_tab_order`). A copy
// lives in a small file that is read before the first screen draws, because the bridge
// is async and the tab bar must not flash the shipped order. The same plan as theme/prefs.ts.
import { File, Paths } from "expo-file-system";
import { useSyncExternalStore } from "react";
import { call } from "../data/source";
import { notifyError } from "../ui/notify-error";
import { coreWins, latestWins, parseTabPrefs, SHIPPED_PREFS, type TabPrefs } from "./tabs";

export type { TabPrefs };
const file = () => new File(Paths.document, "tabs.json");

const read = (): TabPrefs => {
  try {
    return parseTabPrefs(file().textSync());
  } catch {
    // No file yet: the shipped order is a complete tab bar.
    return SHIPPED_PREFS;
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

// One save at a time, and the last tap wins, so the stored order is the one on screen.
const send = latestWins(async (next: TabPrefs) => {
  try {
    await call("set_tab_order", { tabOrder: next.order, startTab: next.start });
  } catch (e) {
    notifyError("Could not save the tabs", e);
  }
});

/** Apply the choice at once, then save it in the core. A failed save shows the reason. */
export function saveTabPrefs(next: TabPrefs) {
  apply(next);
  return send(next);
}

/** The core is the source of truth. Take its value when the copy differs, for example after a restore. */
export async function syncTabPrefs() {
  // An error here is not shown: the copy is a full tab bar, and the next start syncs again.
  const core = await call("prefs").catch(() => null);
  const next = core && coreWins(core, current);
  if (next) apply(next);
}
