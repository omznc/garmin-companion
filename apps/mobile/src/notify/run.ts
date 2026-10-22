// The steps after a sync. Each native or core part comes in as a dependency, so a test can fake it.
import {
  type Facts,
  type NotifyPrefs,
  type NotifyState,
  type Planned,
  planNotify,
} from "./plan.ts";

export type NotifyDeps = {
  prefs: () => NotifyPrefs;
  state: () => NotifyState;
  saveState: (state: NotifyState) => void;
  signedIn: () => Promise<boolean>;
  sample: () => boolean;
  facts: () => Promise<Pick<Facts, "date" | "recent" | "attention">>;
  // True when the person turned notifications on and the system allows them.
  permitted: () => Promise<boolean>;
  deliver: (planned: Planned) => Promise<void>;
};

export async function notifyAfterSync(deps: NotifyDeps, history: boolean): Promise<Planned[]> {
  if (deps.sample() || !(await deps.signedIn())) return [];
  const prefs = deps.prefs();
  // Without the permission the notifications are off. The ids still go in the memory.
  const allowed = { ...prefs, enabled: prefs.enabled && (await deps.permitted()) };
  const { planned, state } = planNotify(allowed, deps.state(), {
    signedIn: true,
    sample: false,
    history,
    ...(await deps.facts()),
  });
  // The memory comes first, so a failed delivery never sends the same notification twice.
  deps.saveState(state);
  for (const p of planned) await deps.deliver(p);
  return planned;
}
