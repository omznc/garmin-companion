// The pure rules of an Ask thread: how an event changes a turn, how the live turn
// covers the stored ones, and which messages a follow-up sends. No React here.
import type { AskEventPayload, HistoryRow, Turn } from "../data/chatTypes.ts";

// The turn that runs now. `index` is the place it has in the thread, which is the
// number of turns stored before it. The core numbers its turn ids the same way.
// `turnId` is the id that the app gave to this run. An event of another run is stale.
export type Live = { index: number; turn: Turn; turnId: string };
export type Lives = ReadonlyMap<string, Live>;

export function newTurnId(): string {
  return `t-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function newSessionId(): string {
  return `s-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function startLive(sessionId: string, index: number, question: string, turnId = ""): Live {
  const turn: Turn = {
    id: `${sessionId}:${index}`,
    question,
    answer: "",
    steps: [],
    status: "running",
    error: null,
  };
  return { index, turn, turnId };
}

// Steps are keyed by call id: a call comes twice, when it starts and when it ends.
export function applyEvent(turn: Turn, event: AskEventPayload): Turn {
  switch (event.type) {
    case "delta":
      return { ...turn, answer: turn.answer + event.text };
    case "tool": {
      const step = {
        callId: event.callId,
        label: event.label,
        running: event.running,
        ok: event.running ? null : event.ok,
      };
      const known = turn.steps.some((s) => s.callId === step.callId);
      const steps = known
        ? turn.steps.map((s) => (s.callId === step.callId ? step : s))
        : [...turn.steps, step];
      return { ...turn, steps };
    }
    case "done":
      return turn.status === "running" ? { ...turn, status: "done" } : turn;
    case "error":
      return { ...turn, status: "error", error: event.text };
  }
}

// The live turn replaces the stored turn of the same place, or follows the stored ones.
// The place, not the id, decides, so a stored copy never shows twice.
export function overlay(stored: Turn[], live: Live | undefined): Turn[] {
  return live ? [...stored.slice(0, live.index), live.turn] : stored;
}

// A done turn gives both messages. A stopped turn gives its question and its partial
// answer, but only when it has one. An error turn and a running turn give nothing.
export function historyFor(stored: Turn[], question: string): HistoryRow[] {
  const rows: HistoryRow[] = [];
  for (const turn of stored) {
    const answered = turn.answer.trim() !== "";
    if (turn.status === "done" || (turn.status === "stopped" && answered)) {
      rows.push({ role: "user", content: turn.question });
      if (answered) rows.push({ role: "assistant", content: turn.answer });
    }
  }
  rows.push({ role: "user", content: question });
  return rows;
}

// The session that has a running turn, or null. One turn runs at a time.
export function runningOf(lives: Lives): string | null {
  for (const [id, live] of lives) if (live.turn.status === "running") return id;
  return null;
}

// A send is refused while a turn of another session runs.
export function canStart(lives: Lives, sessionId: string): boolean {
  const running = runningOf(lives);
  return running === null || running === sessionId;
}

// An event counts only for the current run of its session. A late event of a
// deleted thread, or of an older run, changes nothing.
export function isCurrent(lives: Lives, event: AskEventPayload): boolean {
  return lives.get(event.sessionId)?.turnId === event.turnId;
}

export function reduceEvent(lives: Lives, event: AskEventPayload): Lives {
  const live = lives.get(event.sessionId);
  if (!live || !isCurrent(lives, event)) return lives;
  return new Map(lives).set(event.sessionId, { ...live, turn: applyEvent(live.turn, event) });
}

// The new run joins the live turns. canStart refuses a send while another session runs.
export function startRun(
  lives: Lives,
  sessionId: string,
  index: number,
  question: string,
  turnId: string,
): Lives {
  return new Map(lives).set(sessionId, startLive(sessionId, index, question, turnId));
}

// The saved thread is read: drop the live copy of that run. A newer run that still runs stays.
export function settleRun(lives: Lives, sessionId: string, turnId: string): Lives {
  const live = lives.get(sessionId);
  if (!live || live.turnId !== turnId || live.turn.status === "running") return lives;
  const next = new Map(lives);
  next.delete(sessionId);
  return next;
}

// ask_send failed: the turn was never saved, so drop it. The text goes back to the composer.
export function failRun(lives: Lives, sessionId: string, turnId: string): Lives {
  if (lives.get(sessionId)?.turnId !== turnId) return lives;
  const next = new Map(lives);
  next.delete(sessionId);
  return next;
}
