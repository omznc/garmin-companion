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
  hour: 12,
  recent: [],
  attention: [],
  ...over,
});
const act = (id: number, name: string | null = `Run ${id}`) => ({ id, name });
const state = (activities: number[] | null) => ({ activities });
const note = (kind: string, text = `Note ${kind}.`, accent = true) => ({ accent, text, kind });

test("the default is off", () => {
  assert.equal(DEFAULT_PREFS.enabled, false);
  const r = planNotify(DEFAULT_PREFS, state([1]), facts({ recent: [act(2), act(1)] }));
  assert.deepEqual(r.activities, []);
  // The id is still recorded, so turning the switch on later does not send old news.
  assert.deepEqual(r.state.activities, [2, 1]);
});

test("the first run records the ids and sends nothing, not even a nudge", () => {
  const r = planNotify(
    on,
    EMPTY_STATE,
    facts({ recent: [act(2), act(1)], attention: [note("hrv")] }),
  );
  assert.deepEqual([r.activities, r.nudges], [[], []]);
  assert.deepEqual(r.state.activities, [2, 1]);
});

test("one new activity gives one notification with a link to it", () => {
  const r = planNotify(on, state([1]), facts({ recent: [act(2, "Morning run"), act(1)] }));
  assert.equal(r.activities.length, 1);
  assert.deepEqual(
    [r.activities[0]?.title, r.activities[0]?.body, r.activities[0]?.href],
    ["New activity synced", "Morning run", "/today/activity/2"],
  );
});

test("many new activities give one summary, not one each", () => {
  const r = planNotify(on, state([1]), facts({ recent: [act(4), act(3), act(2), act(1)] }));
  assert.equal(r.activities.length, 1);
  assert.equal(r.activities[0]?.title, "3 new activities synced");
  assert.equal(r.activities[0]?.body, "Run 4, Run 3");
  assert.equal(r.activities[0]?.href, "/activities");
});

test("the same activity is never notified twice", () => {
  const first = planNotify(on, state([1]), facts({ recent: [act(2), act(1)] }));
  const second = planNotify(on, first.state, facts({ recent: [act(2), act(1)] }));
  assert.deepEqual(second.activities, []);
});

test("nameless activities still get a body", () => {
  const one = planNotify(on, state([1]), facts({ recent: [act(2, null), act(1)] }));
  assert.equal(one.activities[0]?.body, "Open it to see the details.");
  const many = planNotify(on, state([1]), facts({ recent: [act(3, " "), act(2, null), act(1)] }));
  assert.equal(many.activities[0]?.body, "Open the list to see them.");
});

test("the activities switch stops activity notifications only", () => {
  const r = planNotify(
    { ...on, activities: false },
    state([1]),
    facts({ recent: [act(2), act(1)], attention: [note("hrv")] }),
  );
  assert.deepEqual(r.activities, []);
  assert.equal(r.nudges.length, 1);
});

test("a full history import is quiet for activities and nudges", () => {
  const r = planNotify(
    on,
    state([1]),
    facts({ history: true, recent: [act(2), act(1)], attention: [note("hrv")] }),
  );
  assert.deepEqual([r.activities, r.nudges], [[], []]);
  assert.deepEqual(r.state.activities, [2, 1]);
});

test("sample mode and a signed-out account change nothing", () => {
  const before = state([1]);
  const recent = [act(2), act(1)];
  const attention = [note("hrv")];
  for (const over of [{ sample: true }, { signedIn: false }]) {
    const r = planNotify(on, before, facts({ ...over, recent, attention }));
    assert.deepEqual([r.activities, r.nudges], [[], []]);
    assert.equal(r.state, before);
  }
});

test("only an accent note is a nudge, with the sentence of the Today screen", () => {
  const r = planNotify(
    on,
    state([]),
    facts({
      attention: [
        note("cadence", "A quiet note.", false),
        note("resting-hr", "Resting heart rate is 6 above your usual."),
      ],
    }),
  );
  assert.equal(r.nudges.length, 1);
  assert.equal(r.nudges[0]?.body, "Resting heart rate is 6 above your usual.");
  assert.equal(r.nudges[0]?.href, "/today");
  assert.equal(r.nudges[0]?.kind, "resting-hr");
});

test("two notes of one insight stay two nudges, and a kind is listed once", () => {
  const r = planNotify(
    on,
    state([]),
    facts({ attention: [note("resting-hr"), note("hrv"), note("hrv", "Other numbers.")] }),
  );
  assert.deepEqual(
    r.nudges.map((n) => [n.id, n.kind]),
    [
      ["nudge-resting-hr", "resting-hr"],
      ["nudge-hrv", "hrv"],
    ],
  );
});

test("a nudge waits while its switch is off", () => {
  const off = planNotify({ ...on, nudges: false }, state([]), facts({ attention: [note("hrv")] }));
  assert.deepEqual(off.nudges, []);
});

test("a nudge goes out only from 08:00 to 20:59 local time", () => {
  const at = (hour: number) =>
    planNotify(on, state([]), facts({ hour, attention: [note("hrv")] })).nudges.length;
  assert.deepEqual([7, 8, 20, 21, 23, 0].map(at), [0, 1, 1, 0, 0, 0]);
});

test("the night does not stop an activity notification", () => {
  const r = planNotify(on, state([1]), facts({ hour: 2, recent: [act(2), act(1)] }));
  assert.equal(r.activities.length, 1);
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
