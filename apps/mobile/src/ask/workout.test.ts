import assert from "node:assert/strict";
import { test } from "node:test";
import type { WorkoutDraft } from "../data/chatTypes.ts";
import { endLabel, estSeconds, flatCount, subline } from "./workout.ts";

const tempo: WorkoutDraft = {
  name: "Tempo",
  sport: "running",
  steps: [
    { type: "exec", kind: "warmup", end: { type: "time", seconds: 600 }, target: { type: "none" } },
    {
      type: "repeat",
      times: 3,
      steps: [
        { kind: "interval", end: { type: "time", seconds: 180 }, target: { type: "none" } },
        { kind: "recovery", end: { type: "time", seconds: 120 }, target: { type: "none" } },
      ],
    },
  ],
};

test("count and time expand the repeats", () => {
  assert.equal(flatCount(tempo), 7);
  assert.equal(estSeconds(tempo), 600 + 3 * 300);
  assert.equal(subline(tempo), "Running · about 25 min · 7 steps");
});

test("a step that is not timed removes the duration", () => {
  const open: WorkoutDraft = {
    ...tempo,
    steps: [{ type: "exec", kind: "interval", end: { type: "distance", metres: 800 }, target: { type: "none" } }],
  };
  assert.equal(estSeconds(open), null);
  assert.equal(subline(open), "Running · 1 step");
});

test("end labels", () => {
  assert.equal(endLabel({ type: "time", seconds: 90 }), "1 min 30 s");
  assert.equal(endLabel({ type: "distance", metres: 800 }), "800 m");
  assert.equal(endLabel({ type: "distance", metres: 5000 }), "5 km");
  assert.equal(endLabel({ type: "lap_button" }), "until lap press");
});
