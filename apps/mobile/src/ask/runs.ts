// The Ask turn in flight. The store lives outside React, so a turn goes on when the
// person leaves the chat. It keeps one live turn for each session and listens to the
// "ask" events of the core once. The core saves the turn itself.
import { useSyncExternalStore } from "react";
import type { AskEventPayload, HistoryRow, Thread } from "../data/chatTypes";
import { queryClient } from "../data/queryClient";
import { callJson, subscribe as subscribeBridge } from "../data/source";
import { applyEvent, type Live, startLive } from "./turns";

export const threadsKey = ["askThreads"] as const;
export const threadKey = (sessionId: string) => ["askThread", sessionId] as const;

const lives = new Map<string, Live>();
const listeners = new Set<() => void>();
let runningId: string | null = null;
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

const patch = (sessionId: string, change: (live: Live) => Live["turn"], soon = false) => {
  const live = lives.get(sessionId);
  if (!live) return;
  lives.set(sessionId, { ...live, turn: change(live) });
  runningId = [...lives].find(([, l]) => l.turn.status === "running")?.[0] ?? null;
  if (soon) notifySoon();
  else notify();
};

// The turn is over: read the saved thread, then drop the live copy. The core saves
// before it sends "done", so the saved copy is the final one.
async function settle(sessionId: string) {
  try {
    const thread = await callJson<Thread | null>("chat_get", { sessionId });
    queryClient.setQueryData(threadKey(sessionId), thread);
    if (lives.get(sessionId)?.turn.status !== "running") lives.delete(sessionId);
    notify();
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
  patch(payload.sessionId, (live) => applyEvent(live.turn, payload), payload.type === "delta");
  if (payload.type === "done" || payload.type === "error") void settle(payload.sessionId);
});

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
    () => runningId,
    () => null,
  );

export async function startTurn(
  sessionId: string,
  index: number,
  question: string,
  history: HistoryRow[],
) {
  // The core stops the old turn. Show it as stopped now, and let its "done" confirm.
  for (const [other, live] of lives) {
    if (other !== sessionId && live.turn.status === "running") {
      patch(other, ({ turn }) => ({ ...turn, status: "stopped" }));
    }
  }
  lives.set(sessionId, startLive(sessionId, index, question));
  runningId = sessionId;
  notify();
  try {
    await callJson("ask_send", { sessionId, history });
    void queryClient.invalidateQueries({ queryKey: threadsKey });
  } catch (error) {
    patch(sessionId, ({ turn }) =>
      applyEvent(turn, { sessionId, type: "error", text: (error as Error).message }),
    );
  }
}

export function stopTurn() {
  void callJson("ask_stop").catch(() => {});
}

export function dropLive(sessionId: string) {
  lives.delete(sessionId);
  if (runningId === sessionId) runningId = null;
  notify();
}
