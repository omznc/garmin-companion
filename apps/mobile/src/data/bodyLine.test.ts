import assert from "node:assert/strict";
import { test } from "node:test";
import type { StrengthView, WeightView } from "./body.ts";
import { bodyLine } from "./bodyLine.ts";

// The views are cast: the test sets only the fields that `bodyLine` reads.
const weight = (v: Partial<WeightView>) => v as WeightView;

test("a page with data gives the value line", () => {
  const view = weight({ empty: null, trendKg: 80.9, changeKg: -0.7 });
  assert.equal(bodyLine("weight", view), "80.9 kg, down 0.7 kg");
  const strength = { empty: null, sessionsText: "6 sessions" } as StrengthView;
  assert.equal(bodyLine("strength", strength), "6 sessions");
});

test("a page with no data gives its empty title", () => {
  const empty = { title: "No weight yet", body: "Log a weigh-in." };
  assert.equal(bodyLine("weight", weight({ empty })), "No weight yet");
});
