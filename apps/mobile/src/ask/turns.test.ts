import assert from "node:assert/strict";
import { test } from "node:test";
import type { Turn } from "../data/chatTypes.ts";
import {
  applyEvent,
  canStart,
  failRun,
  historyFor,
  type Lives,
  overlay,
  reduceEvent,
  runningOf,
  settleRun,
  startLive,
  startRun,
} from "./turns.ts";

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
    turnId: "",
    type: "tool",
    callId: "c",
    label: "L",
    running: true,
    ok: true,
  });
  assert.equal(t.steps[0]?.ok, null);
  t = applyEvent(t, {
    sessionId: "s",
    turnId: "",
    type: "tool",
    callId: "c",
    label: "L",
    running: false,
    ok: true,
  });
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "delta", text: "Hi" });
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "done", sources: [] });
  assert.equal(t.steps.length, 1);
  assert.deepEqual([t.steps[0]?.ok, t.answer, t.status], [true, "Hi", "done"]);
  const failed = applyEvent(startLive("s", 0, "q").turn, {
    sessionId: "s",
    turnId: "",
    type: "error",
    text: "no",
  });
  assert.deepEqual([failed.status, failed.error], ["error", "no"]);
});

const delta = (sessionId: string, turnId: string, text = "x") =>
  ({ sessionId, turnId, type: "delta", text }) as const;

test("an event of another run changes nothing", () => {
  const lives = startRun(new Map(), "s", 0, "q", "run-2");
  assert.equal(reduceEvent(lives, delta("s", "run-1")), lives);
  assert.equal(reduceEvent(lives, delta("gone", "run-2")), lives);
  assert.equal(reduceEvent(lives, delta("s", "run-2")).get("s")?.turn.answer, "x");
});

test("settle drops a finished run and keeps a newer running run", () => {
  let lives: Lives = startRun(new Map(), "s", 0, "q", "run-1");
  lives = reduceEvent(lives, { sessionId: "s", turnId: "run-1", type: "done", sources: [] });
  assert.equal(settleRun(lives, "s", "run-1").has("s"), false);
  const newer = startRun(lives, "s", 1, "q2", "run-2");
  assert.equal(settleRun(newer, "s", "run-1"), newer);
  assert.equal(settleRun(newer, "s", "run-2"), newer);
});

test("a new run replaces the live copy of its own session", () => {
  const first = startRun(new Map(), "a", 0, "q", "run-a");
  assert.equal(runningOf(first), "a");
  const second = startRun(first, "a", 0, "q2", "run-b");
  assert.equal(second.get("a")?.turnId, "run-b");
  assert.equal(second.size, 1);
});

test("a send is refused while another session runs", () => {
  const lives = startRun(new Map(), "a", 0, "q", "run-a");
  assert.deepEqual([canStart(lives, "b"), canStart(lives, "a")], [false, true]);
  assert.equal(canStart(new Map(), "b"), true);
});

test("a failed ask_send drops its own unsaved run only", () => {
  const lives = startRun(new Map(), "s", 0, "q", "run-1");
  assert.equal(failRun(lives, "s", "run-0"), lives);
  const dropped = failRun(lives, "s", "run-1");
  assert.deepEqual([dropped.has("s"), runningOf(dropped)], [false, null]);
});

test("a note moves the round text out of the answer and into the steps", () => {
  let t = startLive("s", 0, "q").turn;
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "delta", text: "Let me look." });
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "note", text: "Let me look." });
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "delta", text: "Rest today." });
  assert.equal(t.answer, "Rest today.");
  assert.deepEqual(
    t.steps.map((s) => [s.kind, s.label]),
    [["note", "Let me look."]],
  );
  const rows = historyFor([{ ...t, status: "done" }], "next");
  assert.equal(rows[1]?.content, "Rest today.");
});

test("a blank note clears the answer and adds no row", () => {
  let t = startLive("s", 0, "q").turn;
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "delta", text: "\n\n" });
  t = applyEvent(t, { sessionId: "s", turnId: "", type: "note", text: "" });
  assert.deepEqual([t.answer, t.steps.length], ["", 0]);
});

test("a draft event adds a card to the live turn, in order", () => {
  const draft = { name: "Easy", sport: "running" as const, steps: [] };
  const ids = { sessionId: "s", turnId: "t" };
  const first = applyEvent(turn("a", "running"), { ...ids, type: "draft", callId: "c1", draft });
  const next = applyEvent(first, { ...ids, type: "draft", callId: "c2", draft });
  assert.deepEqual(
    next.drafts?.map((d) => [d.callId, d.savedWorkoutId]),
    [
      ["c1", null],
      ["c2", null],
    ],
  );
});
