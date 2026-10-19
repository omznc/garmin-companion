import assert from "node:assert/strict";
import { test } from "node:test";
import { greeting } from "./present.ts";

// 07:51 UTC is 09:51 in CEST (UTC+2).
const at = (utc: string, offsetMin: number) => greeting(new Date(`${utc}Z`), offsetMin);

test("uses the local offset, not UTC", () => {
  assert.equal(at("2026-10-19T07:51:00", 120), "Good morning");
  assert.equal(at("2026-10-19T07:51:00", -420), "Good night");
});

test("changes at 05:00, 12:00 and 18:00 local", () => {
  assert.equal(at("2026-10-19T02:59:00", 120), "Good night");
  assert.equal(at("2026-10-19T03:00:00", 120), "Good morning");
  assert.equal(at("2026-10-19T09:59:00", 120), "Good morning");
  assert.equal(at("2026-10-19T10:00:00", 120), "Good afternoon");
  assert.equal(at("2026-10-19T15:59:00", 120), "Good afternoon");
  assert.equal(at("2026-10-19T16:00:00", 120), "Good evening");
});

test("wraps over midnight with a negative offset", () => {
  assert.equal(at("2026-10-19T01:00:00", -120), "Good evening");
  assert.equal(at("2026-10-19T23:30:00", 120), "Good night");
});
