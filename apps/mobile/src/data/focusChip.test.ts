import assert from "node:assert/strict";
import { test } from "node:test";
import { chipState, pickedFocus, utcDay } from "./focusChip.ts";

const TODAY = "2026-10-22";

test("the live focus shows the calendar button", () => {
  assert.deepEqual(chipState(null, TODAY), { live: true, label: "Today" });
  assert.equal(chipState({ from: TODAY, to: TODAY }, TODAY).live, true);
});

test("a past focus shows its label and the clear button", () => {
  const state = chipState({ from: "2026-10-14", to: "2026-10-14" }, TODAY);
  assert.deepEqual(state, { live: false, label: "Wed 14 Oct" });
});

test("a picked range is in order and never after today", () => {
  assert.deepEqual(pickedFocus("2026-10-20", "2026-10-10", TODAY), {
    from: "2026-10-10",
    to: "2026-10-20",
  });
  assert.deepEqual(pickedFocus("2026-10-20", "2026-12-01", TODAY), {
    from: "2026-10-20",
    to: TODAY,
  });
});

test("a start without an end is a single day, and no start is no focus", () => {
  assert.deepEqual(pickedFocus("2026-10-14", null, TODAY), {
    from: "2026-10-14",
    to: "2026-10-14",
  });
  assert.equal(pickedFocus(null, null, TODAY), null);
});

test("a UTC midnight date keeps its day", () => {
  assert.equal(utcDay(new Date("2026-10-14T00:00:00Z")), "2026-10-14");
});
