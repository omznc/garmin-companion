// The pure rules of an Ask thread: how an event changes a turn, how the live turn
// covers the stored ones, and which messages a follow-up sends. No React here.
import type { AskEventPayload, HistoryRow, Turn } from "../data/chatTypes.ts";

// The turn that runs now. `index` is the place it has in the thread, which is the
// number of turns stored before it. The core numbers its turn ids the same way.
export type Live = { index: number; turn: Turn };

export function newSessionId(): string {
  return `s-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;
}

export function startLive(sessionId: string, index: number, question: string): Live {
  const turn: Turn = {
    id: `${sessionId}:${index}`,
    question,
    answer: "",
    steps: [],
    status: "running",
    error: null,
  };
  return { index, turn };
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
