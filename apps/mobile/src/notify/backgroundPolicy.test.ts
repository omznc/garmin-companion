import assert from "node:assert/strict";
import { test } from "node:test";
import { MIN_GAP_MS, shouldRunBackground } from "./backgroundPolicy.ts";

const now = 1_000_000_000_000;
const base = { signedIn: true, sample: false, historyPending: false, lastAt: null, now };

test("the task syncs when signed in and no recent sync exists", () => {
  assert.equal(shouldRunBackground(base), true);
  assert.equal(shouldRunBackground({ ...base, lastAt: now - MIN_GAP_MS }), true);
});

test("the task never runs in sample mode or when signed out", () => {
  assert.equal(shouldRunBackground({ ...base, sample: true }), false);
  assert.equal(shouldRunBackground({ ...base, signedIn: false }), false);
});

test("the task skips while the history import waits, or after a fresh sync", () => {
  assert.equal(shouldRunBackground({ ...base, historyPending: true }), false);
  assert.equal(shouldRunBackground({ ...base, lastAt: now - 1000 }), false);
});

test("a time in the future reads as old", () => {
  assert.equal(shouldRunBackground({ ...base, lastAt: now + 1000 }), true);
});
