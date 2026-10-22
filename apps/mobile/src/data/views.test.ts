import assert from "node:assert/strict";
import { test } from "node:test";
import type { ChartsView } from "./generated/ChartsView.ts";
import type { ExtrasView } from "./generated/ExtrasView.ts";
import type { TilesView } from "./generated/TilesView.ts";
import {
  clockText,
  daysTo,
  presentPlanExtras,
  presentSleepExtras,
  presentStrengthCharts,
} from "./viewsPresent.ts";

// The views are cast: a test sets only the fields that a presenter reads.
const tiles = (v: Partial<TilesView>) =>
  ({
    sleepDebt: null,
    bedWake: null,
    sleepScores: { points: [], average: null },
    ...v,
  }) as TilesView;
const extras = (v: Partial<ExtrasView>) =>
  ({
    date: "2026-10-22",
    planned: [],
    race: null,
    nutrition: { meals: [], goals: null, loggedKcal: null },
    ...v,
  }) as ExtrasView;
const charts = (v: Partial<ChartsView>) =>
  ({ setsPerWeek: [], setsByMuscle: { groups: [], unassignedSets: 0 }, ...v }) as ChartsView;

test("sleep extras are hidden with no data", () => {
  assert.equal(presentSleepExtras(tiles({})), null);
  assert.equal(presentSleepExtras(tiles({ sleepDebt: { nights: 14, debtSecs: 30 } })), null);
  const noScore = { points: [null, null], average: null };
  assert.equal(presentSleepExtras(tiles({ sleepScores: noScore })), null);
});

test("sleep extras give the debt, the times and the scores", () => {
  const r = presentSleepExtras(
    tiles({
      sleepDebt: { nights: 14, debtSecs: 7 * 3600 + 15 * 60 },
      bedWake: { nights: 14, bedMinute: 23 * 60 + 5, wakeMinute: 390, consistencyMins: 22.4 },
      sleepScores: { points: [80, null, 90], average: 85 },
    }),
  );
  assert.equal(r?.debt, "14 nights, 7 h 15 min short");
  assert.deepEqual(r?.facts, [
    ["Bedtime", "23:05"],
    ["Wake time", "06:30"],
    ["Consistency", "±22 min"],
  ]);
  assert.equal(r?.scores?.average, "85");
  assert.equal(r?.scores?.points.length, 3);
});

test("a clock minute wraps at midnight", () => {
  assert.equal(clockText(1440), "00:00");
});

test("days to the race count from the given day", () => {
  assert.equal(daysTo("2026-11-01", "2026-10-22"), 10);
  assert.equal(daysTo("2026-10-22", "2026-10-22"), 0);
});

test("plan extras are hidden with no data", () => {
  assert.deepEqual(presentPlanExtras(extras({})), []);
});

test("plan extras give the race, the planned items and the meals", () => {
  const blocks = presentPlanExtras(
    extras({
      race: {
        eventKey: "k",
        date: "2026-11-01",
        name: "Autumn 10K",
        targetSecs: 2700,
        location: "Sarajevo",
      },
      planned: [
        { title: "Tempo", sport: "running", date: "2026-10-23", steps: { steps: [{}, {}] } },
      ] as ExtrasView["planned"],
      nutrition: {
        meals: [
          { name: "Lunch", time: "12:30", kcal: 650, proteinG: 40, carbsG: null, fatG: null },
        ],
        goals: { calories: 2000, adjustedCalories: 2300, proteinG: null, carbsG: null, fatG: null },
        loggedKcal: 650,
      },
    }),
  );
  const tilesBlock = blocks.find((b) => b.kind === "tiles");
  assert.ok(tilesBlock?.kind === "tiles");
  const values = tilesBlock.tiles.map((t) => t.value);
  assert.ok(values.includes("10 days to go"));
  assert.ok(values.includes("0:45:00"));
  const rows = blocks.filter((b) => b.kind === "rows");
  assert.equal(rows[0]?.kind === "rows" && rows[0].rows[0]?.sub, "running · 2 steps");
  assert.equal(rows[1]?.kind === "rows" && rows[1].rows[0]?.value, "650 kcal");
  const logged = blocks.filter((b) => b.kind === "tiles")[1];
  assert.ok(logged?.kind === "tiles");
  assert.deepEqual(
    logged.tiles.map((t) => t.value),
    ["650 kcal", "2,300 kcal"],
  );
});

test("strength charts are hidden with no data", () => {
  assert.deepEqual(presentStrengthCharts(charts({})), []);
});

test("strength charts give a bar for each group", () => {
  const blocks = presentStrengthCharts(
    charts({
      setsByMuscle: {
        groups: [
          { group: "Chest", sets: 20 },
          { group: "Back", sets: 10 },
        ],
        unassignedSets: 3,
      },
      setsPerWeek: [
        { weekStart: "2026-10-12", isoWeek: "2026-W42", sets: 12 },
        { weekStart: "2026-10-19", isoWeek: "2026-W43", sets: 15 },
      ],
    }),
  );
  const first = blocks[0];
  assert.ok(first?.kind === "rows");
  assert.equal(first.rows[1]?.bar?.fraction, 0.5);
  assert.equal(first.rows[2]?.value, "3 sets");
  assert.equal(blocks[1]?.kind, "chart");
});
