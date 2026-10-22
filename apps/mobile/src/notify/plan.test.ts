import assert from "node:assert/strict";
import { test } from "node:test";
import {
  DEFAULT_PREFS,
  EMPTY_STATE,
  type Facts,
  type NotifyPrefs,
  planNotify,
  safeHref,
} from "./plan.ts";

const on: NotifyPrefs = { enabled: true, activities: true, nudges: true };
const facts = (over: Partial<Facts> = {}): Facts => ({
  signedIn: true,
  sample: false,
  history: false,
  date: "2026-10-22",
  recent: [],
  attention: [],
  ...over,
});
const act = (id: number, name: string | null = `Run ${id}`) => ({ id, name });
const state = (activities: number[] | null, nudges: string[] = []) => ({ activities, nudges });

test("the default is off", () => {
  assert.equal(DEFAULT_PREFS.enabled, false);
  const r = planNotify(DEFAULT_PREFS, state([1]), facts({ recent: [act(2), act(1)] }));
  assert.deepEqual(r.planned, []);
  // The id is still recorded, so turning the switch on later does not send old news.
  assert.deepEqual(r.state.activities, [2, 1]);
});

test("the first run records the ids and sends nothing", () => {
  const r = planNotify(on, EMPTY_STATE, facts({ recent: [act(2), act(1)] }));
  assert.deepEqual(r.planned, []);
  assert.deepEqual(r.state.activities, [2, 1]);
});

test("one new activity gives one notification with a link to it", () => {
  const r = planNotify(on, state([1]), facts({ recent: [act(2, "Morning run"), act(1)] }));
  assert.equal(r.planned.length, 1);
  assert.deepEqual(
    [r.planned[0]?.title, r.planned[0]?.body, r.planned[0]?.href],
    ["New activity synced", "Morning run", "/today/activity/2"],
  );
});

test("many new activities give one summary, not one each", () => {
  const r = planNotify(on, state([1]), facts({ recent: [act(4), act(3), act(2), act(1)] }));
  assert.equal(r.planned.length, 1);
  assert.equal(r.planned[0]?.title, "3 new activities synced");
  assert.equal(r.planned[0]?.body, "Run 4, Run 3");
  assert.equal(r.planned[0]?.href, "/activities");
});

test("the same activity is never notified twice", () => {
  const first = planNotify(on, state([1]), facts({ recent: [act(2), act(1)] }));
  const second = planNotify(on, first.state, facts({ recent: [act(2), act(1)] }));
  assert.deepEqual(second.planned, []);
});

test("a nameless activity still gets a body", () => {
  const r = planNotify(on, state([1]), facts({ recent: [act(2, null), act(1)] }));
  assert.equal(r.planned[0]?.body, "Open it to see the details.");
});

test("the activities switch stops activity notifications only", () => {
  const prefs = { ...on, activities: false };
  const r = planNotify(
    prefs,
    state([1]),
    facts({
      recent: [act(2), act(1)],
      attention: [{ accent: true, text: "Sleep was short." }],
    }),
  );
  assert.deepEqual(
    r.planned.map((p) => p.channel),
    ["nudges"],
  );
});

test("a full history import sends no activity notification", () => {
  const r = planNotify(on, state([1]), facts({ history: true, recent: [act(2), act(1)] }));
  assert.deepEqual(r.planned, []);
  assert.deepEqual(r.state.activities, [2, 1]);
});

test("sample mode and a signed-out account change nothing", () => {
  const before = state([1], ["x"]);
  const recent = [act(2), act(1)];
  const attention = [{ accent: true, text: "Sleep was short." }];
  for (const over of [{ sample: true }, { signedIn: false }]) {
    const r = planNotify(on, before, facts({ ...over, recent, attention }));
    assert.deepEqual(r.planned, []);
    assert.equal(r.state, before);
  }
});

test("only an accent note is a nudge, with the sentence of the Today screen", () => {
  const r = planNotify(
    on,
    state([]),
    facts({
      attention: [
        { accent: false, text: "A quiet note." },
        { accent: true, text: "Resting heart rate is 6 above your usual." },
      ],
    }),
  );
  assert.equal(r.planned.length, 1);
  assert.equal(r.planned[0]?.body, "Resting heart rate is 6 above your usual.");
  assert.equal(r.planned[0]?.href, "/today");
});

test("the same nudge is sent once for a day and again the next day", () => {
  const attention = [{ accent: true, text: "Sleep was short.", insight: "sleep-short" }];
  const one = planNotify(on, state([]), facts({ attention }));
  assert.equal(one.planned.length, 1);
  const again = planNotify(on, one.state, facts({ attention }));
  assert.deepEqual(again.planned, []);
  const tomorrow = planNotify(on, again.state, facts({ attention, date: "2026-10-23" }));
  assert.equal(tomorrow.planned.length, 1);
  // The memory of the old day is gone.
  assert.deepEqual(tomorrow.state.nudges, ["2026-10-23|sleep-short"]);
});

test("a nudge waits while its switch is off", () => {
  const attention = [{ accent: true, text: "Sleep was short." }];
  const off = planNotify({ ...on, nudges: false }, state([]), facts({ attention }));
  assert.deepEqual(off.planned, []);
  assert.deepEqual(off.state.nudges, []);
});

test("at most two nudges go out in one run", () => {
  const attention = ["a", "b", "c"].map((text) => ({ accent: true, text }));
  const r = planNotify(on, state([]), facts({ attention }));
  assert.equal(r.planned.length, 2);
  const next = planNotify(on, r.state, facts({ attention }));
  assert.deepEqual(
    next.planned.map((p) => p.body),
    ["c"],
  );
});

test("a run that finds no activity keeps the memory", () => {
  const r = planNotify(on, state([5, 4]), facts({ recent: [] }));
  assert.deepEqual(r.state.activities, [5, 4]);
});

test("a tap opens only a known screen", () => {
  assert.equal(safeHref("/today"), "/today");
  assert.equal(safeHref("/today/activity/12"), "/today/activity/12");
  assert.equal(safeHref("/sign-in"), null);
  assert.equal(safeHref("/today/activity/x"), null);
  assert.equal(safeHref(undefined), null);
});
