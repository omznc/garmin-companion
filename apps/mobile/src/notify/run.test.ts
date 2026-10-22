import assert from "node:assert/strict";
import { test } from "node:test";
import type { NotifyPrefs, NotifyState, Planned } from "./plan.ts";
import { type NotifyDeps, notifyAfterSync } from "./run.ts";

const on: NotifyPrefs = { enabled: true, activities: true, nudges: true };

function fake(over: Partial<NotifyDeps> = {}) {
  let memory: NotifyState = { activities: [1], nudges: [] };
  const sent: Planned[] = [];
  const deps: NotifyDeps = {
    prefs: () => on,
    state: () => memory,
    saveState: (s) => {
      memory = s;
    },
    signedIn: async () => true,
    sample: () => false,
    facts: async () => ({
      date: "2026-10-22",
      recent: [
        { id: 2, name: "Run" },
        { id: 1, name: "Old" },
      ],
      attention: [],
    }),
    permitted: async () => true,
    deliver: async (p) => {
      sent.push(p);
    },
    ...over,
  };
  return { deps, sent, memory: () => memory };
}

test("a new activity is delivered once", async () => {
  const f = fake();
  await notifyAfterSync(f.deps, false);
  await notifyAfterSync(f.deps, false);
  assert.equal(f.sent.length, 1);
});

test("sample mode reads nothing and sends nothing", async () => {
  const f = fake({
    sample: () => true,
    facts: async () => assert.fail("must not read facts"),
  });
  assert.deepEqual(await notifyAfterSync(f.deps, false), []);
  assert.equal(f.sent.length, 0);
});

test("a signed-out account sends nothing", async () => {
  const f = fake({ signedIn: async () => false });
  await notifyAfterSync(f.deps, false);
  assert.equal(f.sent.length, 0);
});

test("a refused system permission sends nothing but records the ids", async () => {
  const f = fake({ permitted: async () => false });
  await notifyAfterSync(f.deps, false);
  assert.equal(f.sent.length, 0);
  assert.deepEqual(f.memory().activities, [2, 1]);
});

test("a failed delivery does not send the activity again", async () => {
  const f = fake({
    deliver: async () => {
      throw new Error("no channel");
    },
  });
  await assert.rejects(notifyAfterSync(f.deps, false));
  assert.deepEqual(f.memory().activities, [2, 1]);
});
