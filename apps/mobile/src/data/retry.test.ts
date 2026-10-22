import assert from "node:assert/strict";
import { test } from "node:test";
import { isTransient, shouldRetry } from "./retry.ts";

const fault = (message: string, kind?: string) => Object.assign(new Error(message), { kind });

test("retries a transport error once", () => {
  const error = fault("No connection to Garmin.", "offline");
  assert.equal(shouldRetry(0, error), true);
  assert.equal(shouldRetry(1, error), false);
});

test("never retries a refusal or a dead sign-in", () => {
  assert.equal(isTransient(fault("/x returned 429 Too Many Requests: slow", "internal")), false);
  assert.equal(isTransient(fault("/x returned 404 Not Found: no", "internal")), false);
  assert.equal(isTransient(fault("DI token refresh failed: 401 bad", "internal")), false);
  assert.equal(isTransient(fault("Not connected to Garmin yet.", "not_connected")), false);
});

test("does not retry a 5xx, which the core retried", () => {
  assert.equal(isTransient(fault("/x returned 503 Service Unavailable: busy", "internal")), false);
});

test("does not retry a local fault or a value that is not an error", () => {
  assert.equal(isTransient(fault("no such table", "internal")), false);
  assert.equal(isTransient("boom"), false);
});
