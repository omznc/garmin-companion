import assert from "node:assert/strict";
import { test } from "node:test";
import { shouldRetryOffline } from "./syncRetry.ts";

const offline = Object.assign(new Error("No connection to Garmin."), { kind: "offline" });
const other = Object.assign(new Error("boom"), { kind: "internal" });

test("retries an offline failure after the app left the foreground", () => {
  assert.equal(shouldRetryOffline(offline, ["active", "background"]), true);
});

test("does not retry when the app stayed in the foreground", () => {
  assert.equal(shouldRetryOffline(offline, ["active"]), false);
});

test("retries a run that started in the background, such as a queued import pass", () => {
  assert.equal(shouldRetryOffline(offline, ["background"]), true);
});

test("does not retry another kind of failure", () => {
  assert.equal(shouldRetryOffline(other, ["active", "background"]), false);
  assert.equal(shouldRetryOffline("x", ["active", "background"]), false);
});
