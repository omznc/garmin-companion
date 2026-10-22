import assert from "node:assert/strict";
import { test } from "node:test";
import { clipHealth, clipSleep } from "./focusClip.ts";
import type { HealthView } from "./generated/HealthView";
import type { SleepView } from "./generated/SleepView";

const health = {
  date: "2026-10-20",
  days: 5,
  metrics: [{ key: "hrv", points: [1, null, 3, 4, 5] }],
} as unknown as HealthView;

test("a range drops the health days before its start", () => {
  const out = clipHealth(health, { from: "2026-10-18", to: "2026-10-20" });
  assert.deepEqual(out.metrics[0]?.points, [3, 4, 5]);
  assert.equal(out.days, 3);
});

test("a day and the live focus keep the whole health series", () => {
  assert.equal(clipHealth(health, null), health);
  assert.equal(clipHealth(health, { from: "2026-10-18", to: "2026-10-18" }), health);
});

const bar = (date: string) => ({ date, label: date, hours: 7, score: 70 });
const sleep = {
  week: { days: 3, bars: [bar("2026-10-18"), bar("2026-10-19"), bar("2026-10-20")], summary: "" },
  month: { days: 3, bars: [bar("2026-09-01"), bar("2026-10-20")], summary: "" },
} as unknown as SleepView;

test("a range keeps the sleep nights inside it", () => {
  const out = clipSleep(sleep, { from: "2026-10-19", to: "2026-10-20" });
  assert.deepEqual(
    out.week.bars.map((b) => b.date),
    ["2026-10-19", "2026-10-20"],
  );
  assert.equal(out.month.bars.length, 1);
  assert.equal(clipSleep(sleep, null), sleep);
});
