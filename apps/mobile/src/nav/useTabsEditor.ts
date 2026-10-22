// What the Tabs page does. The screens only draw.
import { saveTabPrefs, useTabPrefs } from "./tabPrefs";
import { isShipped, movableTabs, moveTab, orderedTabs, startTab, type TabName } from "./tabs";

export function useTabsEditor() {
  const prefs = useTabPrefs();
  const keep = { order: prefs.order, start: prefs.start };
  return {
    movable: movableTabs(prefs.order),
    /** Every tab in the bar order, for the start tab choice. */
    all: orderedTabs(prefs.order),
    start: startTab(prefs.start),
    shipped: isShipped(prefs.order, prefs.start),
    move: (name: TabName, by: -1 | 1) =>
      saveTabPrefs({ ...keep, order: moveTab(prefs.order, name, by) }),
    chooseStart: (name: TabName) => saveTabPrefs({ ...keep, start: name }),
    reset: () => saveTabPrefs({ order: [], start: "" }),
  };
}
