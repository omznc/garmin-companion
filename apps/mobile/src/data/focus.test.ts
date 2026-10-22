import assert from "node:assert/strict";
import { test } from "node:test";
import {
  clearFocus,
  emptyActivitiesText,
  focusArgs,
  focusKey,
  focusLabel,
  focusPresets,
  getFocus,
  isLive,
  makeFocus,
  setFocus,
  todayIso,
} from "./focus.ts";

const TODAY = "2026-10-20";

test("the label follows the range, like the desktop", () => {
  assert.equal(focusLabel(null, TODAY), "Today");
  assert.equal(focusLabel({ from: TODAY, to: TODAY }, TODAY), "Today");
  assert.equal(focusLabel({ from: "2026-10-14", to: "2026-10-14" }, TODAY), "Wed 14 Oct");
  assert.equal(focusLabel({ from: "2026-10-14", to: "2026-10-20" }, TODAY), "14 - 20 Oct 2026");
  assert.equal(focusLabel({ from: "2026-09-28", to: "2026-10-04" }, TODAY), "28 Sep - 4 Oct 2026");
  assert.equal(
    focusLabel({ from: "2026-12-30", to: "2027-01-05" }, "2027-01-06"),
    "30 Dec 2026 - 5 Jan 2027",
  );
});

test("two dates make an ordered focus and a future date becomes today", () => {
  assert.deepEqual(makeFocus("2026-10-20", "2026-10-14", TODAY), {
    from: "2026-10-14",
    to: "2026-10-20",
  });
  assert.deepEqual(makeFocus("2026-10-18", "2026-11-30", TODAY), {
    from: "2026-10-18",
    to: TODAY,
  });
});

test("the live focus sends no date and a day sends one", () => {
  assert.equal(focusArgs(null), undefined);
  assert.deepEqual(focusArgs({ from: "2026-10-14", to: "2026-10-14" }), {
    from: "2026-10-14",
    to: "2026-10-14",
  });
  assert.equal(focusKey(null), "live");
  assert.equal(focusKey({ from: "2026-10-14", to: "2026-10-20" }), "2026-10-14..2026-10-20");
  assert.ok(isLive({ from: TODAY, to: TODAY }, TODAY));
  assert.ok(!isLive({ from: "2026-10-19", to: TODAY }, TODAY));
});

test("an empty range has its own note", () => {
  assert.match(emptyActivitiesText({ from: "2026-10-14", to: "2026-10-20" }), /in this range/);
  assert.match(emptyActivitiesText(null), /No sessions yet/);
  assert.match(emptyActivitiesText({ from: "2026-10-14", to: "2026-10-14" }), /No sessions yet/);
});

test("the presets end on today", () => {
  const p = focusPresets(TODAY);
  assert.deepEqual(p[1]?.focus, { from: "2026-10-14", to: TODAY });
  assert.deepEqual(p[3]?.focus, { from: "2026-10-01", to: TODAY });
  assert.deepEqual(p[0]?.focus, { from: "2026-10-19", to: "2026-10-19" });
});

test("the store tells a change once and clears", () => {
  let n = 0;
  setFocus({ from: "2026-10-14", to: "2026-10-20" });
  assert.equal(focusKey(getFocus()), "2026-10-14..2026-10-20");
  n += 1;
  setFocus({ from: "2026-10-14", to: "2026-10-20" });
  clearFocus();
  assert.equal(getFocus(), null);
  assert.equal(n, 1);
});

test("a focus on the single day of today is the live focus", () => {
  const today = todayIso();
  setFocus({ from: today, to: today });
  assert.equal(getFocus(), null);
  clearFocus();
});
