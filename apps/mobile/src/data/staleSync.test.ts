import assert from "node:assert/strict";
import { test } from "node:test";
import { STALE_AFTER_MS, syncIsStale } from "./staleSync.ts";

const now = 1_000_000_000_000;

test("a sync that never ran is stale", () => {
  assert.equal(syncIsStale(null, now), true);
});

test("a recent sync is not stale", () => {
  assert.equal(syncIsStale(now - STALE_AFTER_MS + 1, now), false);
});

test("an old sync is stale", () => {
  assert.equal(syncIsStale(now - STALE_AFTER_MS, now), true);
});

test("a time in the future is stale", () => {
  assert.equal(syncIsStale(now + 1, now), true);
});
