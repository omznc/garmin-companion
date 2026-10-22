import assert from "node:assert/strict";
import { test } from "node:test";
import { activitySummary } from "./activitySummary.ts";

const detail = {
  activityId: 1,
  name: "Morning run",
  sportLabel: "Running",
  family: "run",
  when: "Wed 11 Mar, 07:15",
  stats: [
    { label: "Distance", value: "6.4 km" },
    { label: "Time", value: "35 min" },
  ],
  zones: null,
  laps: [],
  strength: null,
  chart: null,
} as unknown as Parameters<typeof activitySummary>[0];

test("the summary has the name, the sport, the time and each figure", () => {
  assert.equal(
    activitySummary(detail),
    "Morning run\nRunning, Wed 11 Mar, 07:15\nDistance: 6.4 km\nTime: 35 min",
  );
});

test("a session with no sport label and no figures is the name and the time", () => {
  assert.equal(
    activitySummary({ ...detail, sportLabel: "", stats: [] }),
    "Morning run\nWed 11 Mar, 07:15",
  );
});
