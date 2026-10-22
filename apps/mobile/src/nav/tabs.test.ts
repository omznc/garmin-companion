import assert from "node:assert/strict";
import { test } from "node:test";
import { isShipped, movableTabs, moveTab, orderedTabs, startTab } from "./tabs.ts";

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
