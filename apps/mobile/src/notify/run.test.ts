import assert from "node:assert/strict";
import { test } from "node:test";
import type { NotifyPrefs, NotifyState, Planned } from "./plan.ts";
import { enableNotifications, type NotifyDeps, notifyAfterSync, notifySafely } from "./run.ts";

const on: NotifyPrefs = { enabled: true, activities: true, nudges: true };
const note = (kind: string) => ({ accent: true, text: `Note ${kind}.`, kind });

function fake(over: Partial<NotifyDeps> = {}) {
  let memory: NotifyState = { activities: [1] };
  const sent: Planned[] = [];
  const claimed = new Set<string>();
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
      hour: 12,
      recent: [
        { id: 2, name: "Run" },
        { id: 1, name: "Old" },
      ],
      attention: [],
    }),
    permitted: async () => true,
    // Like the core: true for the first caller of a kind and day.
    claim: async (kind, date) => {
      const key = `${date}|${kind}`;
      if (claimed.has(key)) return false;
      claimed.add(key);
      return true;
    },
    deliver: async (p) => {
      sent.push(p);
    },
    ...over,
  };
  return { deps, sent, memory: () => memory };
}

const withNotes =
  (...kinds: string[]) =>
  async () => ({
    date: "2026-10-22",
    hour: 12,
    recent: [{ id: 1, name: "Old" }],
    attention: kinds.map(note),
  });

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

test("a failed delivery does not stop the next one and does not repeat", async () => {
  const warn = console.warn;
  console.warn = () => {};
  try {
    const sent: string[] = [];
    const f = fake({
      facts: async () => ({
        date: "2026-10-22",
        hour: 12,
        recent: [
          { id: 2, name: "Run" },
          { id: 1, name: "Old" },
        ],
        attention: [note("hrv")],
      }),
      deliver: async (p) => {
        if (p.channel === "activities") throw new Error("no channel");
        sent.push(p.id);
      },
    });
    const out = await notifyAfterSync(f.deps, false);
    assert.deepEqual([sent, out.map((p) => p.id)], [["nudge-hrv"], ["nudge-hrv"]]);
    assert.deepEqual(f.memory().activities, [2, 1]);
  } finally {
    console.warn = warn;
  }
});

test("a nudge kind goes out once for a day, through the claim", async () => {
  const f = fake({ facts: withNotes("hrv") });
  await notifyAfterSync(f.deps, false);
  await notifyAfterSync(f.deps, false);
  assert.deepEqual(
    f.sent.map((p) => p.id),
    ["nudge-hrv"],
  );
});

test("the resting-rate and HRV notes both go out", async () => {
  const f = fake({ facts: withNotes("resting-hr", "hrv") });
  await notifyAfterSync(f.deps, false);
  assert.deepEqual(
    f.sent.map((p) => p.kind),
    ["resting-hr", "hrv"],
  );
});

test("a claim that is lost or fails sends no nudge, and two nudges are the most", async () => {
  const lost = fake({ facts: withNotes("hrv"), claim: async () => false });
  await notifyAfterSync(lost.deps, false);
  const broken = fake({
    facts: withNotes("hrv"),
    claim: async () => {
      throw new Error("core");
    },
  });
  await notifyAfterSync(broken.deps, false);
  assert.equal(lost.sent.length + broken.sent.length, 0);
  const many = fake({ facts: withNotes("a", "b", "c") });
  await notifyAfterSync(many.deps, false);
  assert.equal(many.sent.length, 2);
});

test("a quiet run claims nothing", async () => {
  const first = fake({
    state: () => ({ activities: null }),
    facts: withNotes("hrv"),
    claim: async () => assert.fail("must not claim"),
  });
  await notifyAfterSync(first.deps, false);
  const history = fake({ facts: withNotes("hrv"), claim: async () => assert.fail("no") });
  await notifyAfterSync(history.deps, true);
  assert.equal(first.sent.length + history.sent.length, 0);
});

test("a night run claims no nudge, so the morning can send it", async () => {
  const f = fake({
    facts: async () => ({ ...(await withNotes("hrv")()), hour: 23 }),
    claim: async () => assert.fail("must not claim"),
  });
  await notifyAfterSync(f.deps, false);
  assert.equal(f.sent.length, 0);
});

test("a notify error never fails a sync", async () => {
  const warn = console.warn;
  console.warn = () => {};
  try {
    const bad = fake({
      facts: async () => {
        throw new Error("core down");
      },
    });
    await notifySafely(bad.deps, false);
    const worse = fake({
      deliver: async () => {
        throw new Error("no channel");
      },
    });
    await notifySafely(worse.deps, false);
  } finally {
    console.warn = warn;
  }
});

function permission(granted: boolean, answer: boolean) {
  const calls: string[] = [];
  return {
    calls,
    deps: {
      ensureChannels: async () => void calls.push("channels"),
      granted: async () => (calls.push("granted"), granted),
      request: async () => (calls.push("request"), answer),
      setEnabled: (on: boolean) => void calls.push(`enabled:${on}`),
    },
  };
}

test("the permission prompt shows only when the switch goes on and it is not granted", async () => {
  const known = permission(true, false);
  assert.equal(await enableNotifications(known.deps), true);
  assert.deepEqual(known.calls, ["channels", "granted", "enabled:true"]);
  const fresh = permission(false, true);
  assert.equal(await enableNotifications(fresh.deps), true);
  assert.ok(fresh.calls.includes("request"));
});

test("a refusal keeps the switch off", async () => {
  const p = permission(false, false);
  assert.equal(await enableNotifications(p.deps), false);
  assert.equal(p.calls.at(-1), "enabled:false");
});
