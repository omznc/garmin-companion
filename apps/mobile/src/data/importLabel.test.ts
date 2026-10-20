import assert from "node:assert/strict";
import { test } from "node:test";
import { importLabel } from "./importLabel.ts";

const base = { importing: true, full: true, phase: "wellness", done: 90, total: 365, line: "x" };

test("no row when the import is not running", () => {
  assert.equal(importLabel({ ...base, importing: false }), null);
});

test("the quick pass names the last 14 days", () => {
  assert.equal(importLabel({ ...base, full: false }), "Getting your last 14 days");
});

test("the wellness phase counts months from the total", () => {
  assert.equal(importLabel(base), "Your history · 3 of 12 months");
  assert.equal(importLabel({ ...base, done: 400 }), "Your history · 12 of 12 months");
  assert.equal(importLabel({ ...base, done: 4, total: 4 }), "Your history · 0 of 1 month");
});

test("other phases show the line", () => {
  assert.equal(importLabel({ ...base, phase: "activities" }), "x");
});
