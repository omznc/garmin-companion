import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { greeting, metricRoute, presentToday } from "./present.ts";

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

const fixture = (name: string) =>
  JSON.parse(
    readFileSync(
      new URL(`../../../../crates/garmin-api/fixtures/${name}`, import.meta.url),
      "utf8",
    ),
  );

test("every Today health tile opens a metric that Health sends", () => {
  const today = presentToday(fixture("today.sample.json"));
  const keys = fixture("health.sample.json").metrics.map((m: { key: string }) => m.key);
  const tiles = today.sections.flatMap((s) => (s.kind === "health" ? s.metrics : []));
  assert.ok(tiles.length > 0);
  for (const { id } of tiles.filter((t) => t.id !== "sleep" && t.id !== "readiness")) {
    const key = metricRoute(id).replace("/today/metric/", "");
    assert.ok(keys.includes(key), `${id} -> ${key}`);
  }
});

test("routes sleep to its screen and other ids to Health", () => {
  assert.equal(metricRoute("sleep"), "/today/sleep");
  assert.equal(metricRoute("readiness"), "/health");
  assert.equal(metricRoute("unknown"), "/health");
});
