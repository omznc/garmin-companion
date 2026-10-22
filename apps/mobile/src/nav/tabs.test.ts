import assert from "node:assert/strict";
import { test } from "node:test";
import {
  coreWins,
  isShipped,
  latestWins,
  launchScreens,
  movableTabs,
  moveTab,
  orderedTabs,
  parseTabPrefs,
  SHIPPED_PREFS,
  startTab,
} from "./tabs.ts";

const names = (tabs: { name: string }[]) => tabs.map((t) => t.name);

test("no saved order gives the shipped order, with More last", () => {
  assert.deepEqual(names(orderedTabs([])), ["today", "activities", "health", "sleep", "more"]);
});

test("a saved order moves the tabs, and More stays last", () => {
  assert.deepEqual(names(orderedTabs(["sleep", "today", "health", "activities"])), [
    "sleep",
    "today",
    "health",
    "activities",
    "more",
  ]);
});

test("an unknown key, a repeated key and More are dropped, and a missing tab follows", () => {
  assert.deepEqual(names(movableTabs(["health", "gear", "health", "more"])), [
    "health",
    "today",
    "activities",
    "sleep",
  ]);
});

test("a tab moves one place, and a move past an end changes nothing", () => {
  assert.deepEqual(moveTab([], "health", -1), ["today", "health", "activities", "sleep"]);
  assert.deepEqual(moveTab([], "today", -1), ["today", "activities", "health", "sleep"]);
  assert.deepEqual(moveTab([], "sleep", 1), ["today", "activities", "health", "sleep"]);
  assert.deepEqual(moveTab([], "more", -1), ["today", "activities", "health", "sleep"]);
});

test("the start tab falls back to Today", () => {
  assert.equal(startTab("sleep").name, "sleep");
  assert.equal(startTab("more").name, "more");
  assert.equal(startTab("").name, "today");
  assert.equal(startTab("gear").name, "today");
});

test("the shipped state has no change in the order and Today as the start", () => {
  assert.equal(isShipped([], ""), true);
  assert.equal(isShipped(["today", "activities", "health", "sleep"], "today"), true);
  assert.equal(isShipped(["sleep"], ""), false);
  assert.equal(isShipped([], "health"), false);
});

test("the core wins when it differs from the copy, else nothing changes", () => {
  const copy = { order: ["sleep", "today", "health", "activities"], start: "sleep" };
  assert.equal(coreWins({ tab_order: copy.order, start_tab: "sleep" }, copy), null);
  assert.deepEqual(coreWins({ tab_order: [], start_tab: "" }, copy), { order: [], start: "" });
  assert.deepEqual(coreWins({ tab_order: copy.order, start_tab: "more" }, copy), {
    order: copy.order,
    start: "more",
  });
});

test("the launch screens put the start tab first and keep the saved order", () => {
  const order = ["sleep", "today", "health", "activities"];
  assert.deepEqual(names(launchScreens({ order, start: "health" })), [
    "health",
    "sleep",
    "today",
    "activities",
    "more",
  ]);
  assert.deepEqual(names(launchScreens({ order: [], start: "more" })), [
    "more",
    "today",
    "activities",
    "health",
    "sleep",
  ]);
  assert.equal(launchScreens({ order: [], start: "" })[0]?.name, "today");
});

test("tabs.json parses to the shipped values when it is missing or broken", () => {
  assert.deepEqual(parseTabPrefs('{"order":["sleep",3,"today"],"start":"health"}'), {
    order: ["sleep", "today"],
    start: "health",
  });
  assert.deepEqual(parseTabPrefs("{"), SHIPPED_PREFS);
  assert.deepEqual(parseTabPrefs("null"), SHIPPED_PREFS);
  assert.deepEqual(parseTabPrefs('{"order":"x","start":1}'), SHIPPED_PREFS);
});

test("saves run one at a time and the last value wins", async () => {
  const sent: number[] = [];
  let release = () => {};
  const send = latestWins(async (v: number) => {
    sent.push(v);
    if (v === 1) await new Promise<void>((r) => (release = r));
  });
  const first = send(1);
  await send(2);
  await send(3);
  assert.deepEqual(sent, [1]);
  release();
  await first;
  assert.deepEqual(sent, [1, 3]);
});
