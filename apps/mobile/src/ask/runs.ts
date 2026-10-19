// The Ask turn in flight. The store lives outside React, so a turn goes on when the
// person leaves the chat. It keeps one live turn for each session and listens to the
// "ask" events of the core once. The core saves the turn itself.
import { useSyncExternalStore } from "react";
import type { AskEventPayload, HistoryRow, Thread } from "../data/chatTypes";
import { queryClient } from "../data/queryClient";
import { callJson, subscribe as subscribeBridge } from "../data/source";
import {
  canStart,
  failRun,
  isCurrent,
  type Live,
  type Lives,
  newTurnId,
  reduceEvent,
  runningOf,
  settleRun,
  startRun,
} from "./turns";

export const threadsKey = ["askThreads"] as const;
export const threadKey = (sessionId: string) => ["askThread", sessionId] as const;

let lives: Lives = new Map();
const listeners = new Set<() => void>();
// The frame that will tell the screens about the stream, if one waits.
let frame: ReturnType<typeof requestAnimationFrame> | null = null;

const tell = () => {
  for (const listener of listeners) listener();
};

const notify = () => {
  if (frame !== null) {
    cancelAnimationFrame(frame);
    frame = null;
  }
  tell();
};

// A model can send many deltas in one frame, and the screen can show only one of them.
const notifySoon = () => {
  if (frame !== null) return;
  frame = requestAnimationFrame(() => {
    frame = null;
    tell();
  });
};

const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

const setLives = (next: Lives, soon = false) => {
  if (next === lives) return;
  lives = next;
  if (soon) notifySoon();
  else notify();
};

// The turn is over: read the saved thread, then drop the live copy. The core saves
// before it sends "done", so the saved copy is the final one.
async function settle(sessionId: string, turnId: string) {
  try {
    const thread = await callJson<Thread | null>("chat_get", { sessionId });
    queryClient.setQueryData(threadKey(sessionId), thread);
    setLives(settleRun(lives, sessionId, turnId));
  } catch {
    // The live copy stays, so the answer stays on screen.
  }
  void queryClient.invalidateQueries({ queryKey: threadsKey });
  // A turn that ends has spent credits: read the balance again.
  void queryClient.invalidateQueries({ queryKey: ["cloudAccount"] });
}

subscribeBridge((event) => {
  if (event.kind !== "ask") return;
  const payload = event.payload as AskEventPayload;
  if (!isCurrent(lives, payload)) return;
  setLives(reduceEvent(lives, payload), payload.type === "delta");
  if (payload.type === "done" || payload.type === "error") {
    void settle(payload.sessionId, payload.turnId);
  }
});

// Why the last ask_send failed. It clears on the next send, or after a few seconds.
let sendError: string | null = null;
let errorTimer: ReturnType<typeof setTimeout> | undefined;

const setSendError = (message: string | null) => {
  clearTimeout(errorTimer);
  sendError = message;
  if (message !== null) errorTimer = setTimeout(() => setSendError(null), 6000);
  tell();
};

export const useSendError = (): string | null =>
  useSyncExternalStore(
    subscribe,
    () => sendError,
    () => null,
  );

export const hasLive = (sessionId: string) => lives.has(sessionId);

export const useLive = (sessionId: string | null): Live | undefined =>
  useSyncExternalStore(
    subscribe,
    () => (sessionId === null ? undefined : lives.get(sessionId)),
    () => undefined,
  );

// The session that answers now, or null. One turn runs at a time, in the whole app.
export const useRunningId = (): string | null =>
  useSyncExternalStore(
    subscribe,
    () => runningOf(lives),
    () => null,
  );

// True when the core took the message. False when another chat answers, or ask_send
// failed: the unsaved live turn is dropped, and the composer takes the text back.
export async function startTurn(
  sessionId: string,
  index: number,
  question: string,
  history: HistoryRow[],
): Promise<boolean> {
  if (!canStart(lives, sessionId)) return false;
  const turnId = newTurnId();
  setSendError(null);
  setLives(startRun(lives, sessionId, index, question, turnId));
  try {
    await callJson("ask_send", { sessionId, turnId, history });
  } catch (error) {
    setLives(failRun(lives, sessionId, turnId));
    setSendError(error instanceof Error ? error.message : String(error));
    return false;
  }
  void queryClient.invalidateQueries({ queryKey: threadsKey });
  return true;
}

export function stopTurn() {
  void callJson("ask_stop").catch(() => {});
}

export function dropLive(sessionId: string) {
  const next = new Map(lives);
  next.delete(sessionId);
  setLives(next);
}
