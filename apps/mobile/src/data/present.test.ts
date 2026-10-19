import assert from "node:assert/strict";
import { test } from "node:test";
import { greeting } from "./present.ts";

// Local constructors, so the result does not depend on the time zone of the machine.
const at = (h: number, m: number) => greeting(new Date(2026, 9, 19, h, m));

test("changes at 05:00, 12:00 and 18:00 local", () => {
  assert.equal(at(0, 0), "Good night");
  assert.equal(at(4, 59), "Good night");
  assert.equal(at(5, 0), "Good morning");
  assert.equal(at(9, 51), "Good morning");
  assert.equal(at(11, 59), "Good morning");
  assert.equal(at(12, 0), "Good afternoon");
  assert.equal(at(17, 59), "Good afternoon");
  assert.equal(at(18, 0), "Good evening");
  assert.equal(at(23, 59), "Good evening");
});
