import assert from "node:assert/strict";
import { test } from "node:test";
import type { Turn } from "../data/chatTypes.ts";
import { applyEvent, historyFor, overlay, startLive } from "./turns.ts";

const turn = (id: string, status: Turn["status"], answer = ""): Turn => ({
  id,
  question: `q-${id}`,
  answer,
  steps: [],
  status,
  error: status === "error" ? "boom" : null,
});

test("history keeps done turns and a non-empty stopped turn", () => {
  const stored = [
    turn("a", "done", "A"),
    turn("b", "error"),
    turn("c", "stopped", "part"),
    turn("d", "stopped", "  "),
    turn("e", "running", "x"),
  ];
  assert.deepEqual(historyFor(stored, "now"), [
    { role: "user", content: "q-a" },
    { role: "assistant", content: "A" },
    { role: "user", content: "q-c" },
    { role: "assistant", content: "part" },
    { role: "user", content: "now" },
  ]);
});

test("the live turn replaces the stored turn of its place", () => {
  const stored = [turn("s:0", "done", "A"), turn("s:1", "running", "old")];
  const live = startLive("s", 1, "q");
  assert.deepEqual(
    overlay(stored, live).map((t) => t.id),
    ["s:0", "s:1"],
  );
  assert.equal(overlay(stored, live)[1], live.turn);
});

test("the live turn follows the stored turns when they lack it", () => {
  const stored = [turn("s:0", "done", "A")];
  assert.deepEqual(
    overlay(stored, startLive("s", 1, "q")).map((t) => t.id),
    ["s:0", "s:1"],
  );
  assert.equal(overlay(stored, undefined), stored);
});

test("events build the turn: steps by call id, text, end", () => {
  let t = startLive("s", 0, "q").turn;
  t = applyEvent(t, {
    sessionId: "s",
    type: "tool",
    callId: "c",
    label: "L",
    running: true,
    ok: true,
  });
  assert.equal(t.steps[0]?.ok, null);
  t = applyEvent(t, {
    sessionId: "s",
    type: "tool",
    callId: "c",
    label: "L",
    running: false,
    ok: true,
  });
  t = applyEvent(t, { sessionId: "s", type: "delta", text: "Hi" });
  t = applyEvent(t, { sessionId: "s", type: "done", sources: [] });
  assert.equal(t.steps.length, 1);
  assert.deepEqual([t.steps[0]?.ok, t.answer, t.status], [true, "Hi", "done"]);
  const failed = applyEvent(startLive("s", 0, "q").turn, {
    sessionId: "s",
    type: "error",
    text: "no",
  });
  assert.deepEqual([failed.status, failed.error], ["error", "no"]);
});
