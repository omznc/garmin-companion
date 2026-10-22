// The steps after a sync. Each native or core part comes in as a dependency, so a test can fake it.
import {
  type Facts,
  NUDGES_PER_RUN,
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
  facts: () => Promise<Pick<Facts, "date" | "hour" | "recent" | "attention">>;
  // True when the person turned notifications on and the system allows them.
  permitted: () => Promise<boolean>;
  // True for the one caller that claims the notification of a kind for a day. The core keeps it.
  claim: (kind: string, date: string) => Promise<boolean>;
  deliver: (planned: Planned) => Promise<void>;
};

export async function notifyAfterSync(deps: NotifyDeps, history: boolean): Promise<Planned[]> {
  if (deps.sample() || !(await deps.signedIn())) return [];
  const prefs = deps.prefs();
  // Without the permission the notifications are off. The ids still go in the memory.
  const allowed = { ...prefs, enabled: prefs.enabled && (await deps.permitted()) };
  const facts = await deps.facts();
  const plan = planNotify(allowed, deps.state(), {
    signedIn: true,
    sample: false,
    history,
    ...facts,
  });
  // The memory comes first, so a failed delivery never sends the same notification twice.
  deps.saveState(plan.state);
  const planned = [...plan.activities];
  let nudges = 0;
  for (const n of plan.nudges) {
    if (nudges >= NUDGES_PER_RUN) break;
    // A claim that fails is a nudge that does not go out.
    if (n.kind && (await deps.claim(n.kind, facts.date).catch(() => false))) {
      planned.push(n);
      nudges += 1;
    }
  }
  // One failed delivery must not lose the notifications that the core already claimed.
  const delivered: Planned[] = [];
  for (const p of planned) {
    try {
      await deps.deliver(p);
      delivered.push(p);
    } catch (e) {
      console.warn("notification not delivered", p.id, e);
    }
  }
  return delivered;
}

// A notification must never fail a sync, and the background task waits for it before it ends.
export async function notifySafely(deps: NotifyDeps, history: boolean): Promise<void> {
  try {
    await notifyAfterSync(deps, history);
  } catch (e) {
    console.warn("notification failed", e);
  }
}

export type EnableDeps = {
  ensureChannels: () => Promise<void>;
  granted: () => Promise<boolean>;
  request: () => Promise<boolean>;
  setEnabled: (on: boolean) => void;
};

// Ask for the permission only when the person turns the master switch on.
// Returns false when the system refuses, and then the switch stays off.
export async function enableNotifications(d: EnableDeps): Promise<boolean> {
  await d.ensureChannels();
  const ok = (await d.granted()) || (await d.request());
  d.setEnabled(ok);
  return ok;
}
