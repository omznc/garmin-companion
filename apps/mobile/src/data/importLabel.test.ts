import assert from "node:assert/strict";
import { test } from "node:test";
import { importLabel, QUICK_DAYS } from "./importLabel.ts";

const base = {
  importing: true,
  importPass: "full" as "quick" | "full" | null,
  phase: "wellness",
  done: 90,
  total: 365,
  line: "x",
};

test("no row when the import is not running", () => {
  assert.equal(importLabel({ ...base, importing: false }), null);
});

test("the quick pass names the last days", () => {
  assert.equal(
    importLabel({ ...base, importPass: "quick" }),
    `Getting your last ${QUICK_DAYS} days`,
  );
});

test("the label follows the import pass, not the sync phase", () => {
  const quick = { ...base, importing: true, importPass: "quick" as const, phase: "wellness" };
  assert.equal(importLabel(quick), `Getting your last ${QUICK_DAYS} days`);
  assert.equal(importLabel({ ...quick, importPass: "full" }), "Your history · 3 of 12 months");
});

test("the wellness phase counts months from the total", () => {
  assert.equal(importLabel(base), "Your history · 3 of 12 months");
  assert.equal(importLabel({ ...base, done: 400 }), "Your history · 12 of 12 months");
  assert.equal(importLabel({ ...base, done: 4, total: 4 }), "Your history · 0 of 1 month");
});

test("other phases show the line", () => {
  assert.equal(importLabel({ ...base, phase: "activities" }), "x");
});
