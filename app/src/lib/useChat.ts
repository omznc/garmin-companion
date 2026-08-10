/**
 * One conversation: the turn loop, the transcript, and everything a turn can
 * put on screen while it runs.
 *
 * Both chat surfaces run on this — the Ask screen and the strip at the bottom of
 * an activity page — because they were the same two hundred lines twice, and the
 * copies had already drifted: only one of them knew about drafted workouts. A
 * turn is delicate enough (one event channel per turn, a session created lazily,
 * persistence that must not be able to fail a turn that already succeeded) that
 * having two of it is having one of it wrong.
 *
 * What the screens keep for themselves is everything visual: the transcript's
 * shape, where the composer sits, whether there is a suggestion row. This owns
 * the state and the wire, and has no opinion about any of that.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useQueryClient } from "@tanstack/react-query";
import {
  chatAnswer,
  chatCancel,
  chatFollowups,
  chatSend,
  chatSession,
  saveChatSession,
  type AskOption,
  type AskRecord,
  type ChatMessage,
  type TurnBlock,
  type WorkoutDraft,
} from "./api";

/** What arrives on `chat:{id}`. Mirrors `chat::Event` on the Rust side. */
type ChatEvent =
  | { type: "tool"; callId: string; label: string; running: boolean; ok: boolean }
  | { type: "delta"; text: string }
  | { type: "draft"; draft: WorkoutDraft }
  | {
      type: "ask";
      callId: string;
      header: string | null;
      question: string;
      options: AskOption[];
      multi: boolean;
    }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string };

/**
 * One tool call, live.
 *
 * Kept as a list rather than as the single replaceable line this used to be,
 * because the sequence is the interesting part: which of your data it went to,
 * in what order, and how long each took. Only the turn in flight has these —
 * once it lands, the labels are what persists, on the message.
 */
export interface ToolStep {
  callId: string;
  label: string;
  running: boolean;
  ok: boolean;
  /** Wall clock, for the running seconds beside a slow one. */
  startedAt: number;
  endedAt?: number;
}

/**
 * One thing the turn in flight has done, in the order it did it.
 *
 * The live counterpart of `ChatMessage.blocks`, and the reason the two shapes
 * differ is what they have to point at. While the turn runs a tool row is a
 * moving thing — running, then done, with a clock on it — so a block names the
 * call and the row is looked up in `steps`; a question is looked up the same way
 * because its answer arrives later. Once the turn lands none of that moves any
 * more, and the saved block carries the label itself.
 *
 * Prose accumulates into the last block when it is already text, so a paragraph
 * interrupted by nothing stays one paragraph. A tool call, a question or a
 * drafted workout arriving is what closes it: the next word after one of those
 * starts a new block, below it, which is the whole point of this list.
 */
export type LiveBlock =
  | { kind: "text"; text: string }
  | { kind: "tool"; callId: string }
  | { kind: "ask"; callId: string }
  | { kind: "draft"; index: number };

/** A question the model is waiting on an answer to, right now. */
export interface PendingAsk {
  callId: string;
  header?: string;
  question: string;
  options: AskOption[];
  multi: boolean;
}

export interface Chat {
  /** The transcript as it will be saved. The turn in flight is not in here. */
  history: ChatMessage[];
  /**
   * Whether a turn is running, and everything it has written so far.
   *
   * No longer what the thread draws — `blocks` is, because the prose comes in
   * more than one piece and where the pieces fall matters. This stays as the
   * flag every screen already gates on, and as the answer's full text.
   */
  pending: string | null;
  /** What the turn has done so far, in order. The thread draws this. */
  blocks: LiveBlock[];
  /** Tool calls this turn, oldest first. Cleared when the next turn starts. */
  steps: ToolStep[];
  /** Workouts drafted this turn, before they are part of the transcript. */
  drafting: WorkoutDraft[];
  /** Questions asked this turn — the last one may still be unanswered. */
  asking: Array<PendingAsk & { answers: string[] | null }>;
  error: string | null;
  busy: boolean;
  /** Three things worth asking next, once an answer has landed. */
  followups: string[];
  /** The open conversation's id, or null before the first question. */
  sessionId: string | null;
  send: (text: string) => Promise<void>;
  /** Stop the turn in flight, keeping whatever it has already written. */
  stop: () => void;
  answer: (callId: string, answers: string[]) => Promise<void>;
  /** Start a new, empty conversation. */
  reset: () => void;
  /** Reopen a saved one and keep writing into it. */
  load: (sessionId: string) => Promise<void>;
  /** Record that a proposed workout is now a real one on the Garmin account. */
  markSaved: (messageIndex: number, draftIndex: number, workoutId: number) => void;
  /** The same, for a card belonging to the turn still in flight. */
  markDrafting: (draftIndex: number, workoutId: number) => void;
}

/** Short, unique enough for a channel name and a row id. */
const rid = () => `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;

/**
 * The turn's running order, resolved into the form that goes in the transcript.
 *
 * Everything a live block points at by id is looked up here, once, while the
 * things it points at are still in hand. A block whose target has gone is
 * dropped rather than saved broken — that only happens to a question the turn
 * never got as far as registering, and half a question in a transcript is worse
 * than none.
 */
function persistedBlocks(
  live: LiveBlock[],
  labels: Map<string, { label: string; ok: boolean }>,
  asked: Array<{ callId: string }>,
): TurnBlock[] {
  return live.flatMap<TurnBlock>((b) => {
    if (b.kind === "text") {
      // Trimmed here and nowhere else: the whitespace around a paragraph is an
      // artefact of where the tool call interrupted it, and the blank line that
      // replaces it is put back when the blocks are joined.
      const text = b.text.trim();
      return text ? [{ kind: "text", text }] : [];
    }
    if (b.kind === "tool") {
      const seen = labels.get(b.callId);
      return seen ? [{ kind: "tool", label: seen.label, ok: seen.ok }] : [];
    }
    if (b.kind === "ask") {
      const index = asked.findIndex((a) => a.callId === b.callId);
      return index < 0 ? [] : [{ kind: "ask", index }];
    }
    return [{ kind: "draft", index: b.index }];
  });
}

/** What a conversation is "about": the question that started it. */
export function title(messages: ChatMessage[]): string {
  const first = messages.find((m) => m.role === "user")?.content.trim() ?? "Untitled";
  return first.length > 120 ? `${first.slice(0, 119)}…` : first;
}

export function useChat(options?: {
  /** Scopes every turn to one session, as the activity page does. */
  activityId?: number;
  /** Overrides the saved conversation's title — the activity page names the run. */
  titleFor?: (messages: ChatMessage[]) => string;
  /** Whether to ask for follow-up suggestions after an answer. */
  followups?: boolean;
}): Chat {
  const qc = useQueryClient();
  const { activityId, titleFor, followups: wantFollowups = true } = options ?? {};

  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [pending, setPending] = useState<string | null>(null);
  const [blocks, setBlocks] = useState<LiveBlock[]>([]);
  const [steps, setSteps] = useState<ToolStep[]>([]);
  const [drafting, setDrafting] = useState<WorkoutDraft[]>([]);
  const [asking, setAsking] = useState<Array<PendingAsk & { answers: string[] | null }>>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [followups, setFollowups] = useState<string[]>([]);

  /* The conversation being written to. Created lazily on the first question so
   * that opening the screen and leaving doesn't litter the cache with empties. */
  const session = useRef<{ id: string; startedAt: string } | null>(null);
  const [sessionId, setSessionId] = useState<string | null>(null);
  /** The turn in flight, for Stop and for answering a question it asked. */
  const turn = useRef<string | null>(null);

  /**
   * The turn's questions, in a ref as well as in state.
   *
   * They are written from two places that can't see each other — the event
   * handler inside `send`'s closure puts a question up, and `answer` fills in
   * what was picked — and read from a third, the moment the turn lands and the
   * pair has to go into the transcript. State alone can't serve that read: an
   * updater runs on the next render, which is after the message was built.
   */
  const asks = useRef<Array<PendingAsk & { answers: string[] | null }>>([]);

  const setAsks = useCallback((next: Array<PendingAsk & { answers: string[] | null }>) => {
    asks.current = next;
    setAsking(next);
  }, []);

  /** The turn's drafted workouts, kept the same way and for the same reason. */
  const drafts = useRef<WorkoutDraft[]>([]);

  const setDrafts = useCallback((next: WorkoutDraft[]) => {
    drafts.current = next;
    setDrafting(next);
  }, []);

  /**
   * The running order, in a ref as well as in state, for the third reason in the
   * list above: the message is built the moment the turn lands, and the order it
   * happened in has to be in that message.
   *
   * Appending through a ref rather than a state updater also keeps the deltas
   * honest. Prose arrives a token at a time and a tool event can land between
   * two of them, so "is the last block still text?" has to be asked of what is
   * there now, not of what the last render saw.
   */
  const timeline = useRef<LiveBlock[]>([]);

  const pushBlock = useCallback((block: LiveBlock) => {
    timeline.current = [...timeline.current, block];
    setBlocks(timeline.current);
  }, []);

  /** A token of prose, onto the open paragraph or into a new one under it. */
  const pushText = useCallback((text: string) => {
    const last = timeline.current[timeline.current.length - 1];
    timeline.current =
      last?.kind === "text"
        ? [...timeline.current.slice(0, -1), { kind: "text", text: last.text + text }]
        : [...timeline.current, { kind: "text", text }];
    setBlocks(timeline.current);
  }, []);

  /**
   * A workout sent to Garmin from a card that is still part of the turn in
   * flight, before the transcript has a message to write it into.
   *
   * Without this the id lands nowhere: the turn finishes a second later, the
   * card is redrawn from history without it, and it offers to send a session
   * that is already on the watch.
   */
  const markDrafting = useCallback(
    (draftIndex: number, workoutId: number) => {
      setDrafts(
        drafts.current.map((d, i) => (i === draftIndex ? { ...d, savedWorkoutId: workoutId } : d)),
      );
    },
    [setDrafts],
  );

  const clearTurn = useCallback(() => {
    setPending(null);
    timeline.current = [];
    setBlocks([]);
    setSteps([]);
    setDrafts([]);
    setAsks([]);
    setFollowups([]);
    setError(null);
  }, [setAsks, setDrafts]);

  const reset = useCallback(() => {
    session.current = null;
    setSessionId(null);
    setHistory([]);
    clearTurn();
  }, [clearTurn]);

  // Opening a different activity is a different conversation. Without this,
  // navigating between two runs would carry the first one's transcript onto the
  // second one's page, under the second one's numbers.
  useEffect(() => {
    if (activityId !== undefined) reset();
  }, [activityId, reset]);

  const load = useCallback(
    async (id: string) => {
      const s = await chatSession(id);
      if (!s) return;
      session.current = { id: s.sessionId, startedAt: s.startedAt };
      setSessionId(s.sessionId);
      setHistory(JSON.parse(s.messages) as ChatMessage[]);
      clearTurn();
    },
    [clearTurn],
  );

  /** Write the transcript back to the cache. Never allowed to fail a turn. */
  const persist = useCallback(
    (messages: ChatMessage[]) => {
      const conv = session.current;
      if (!conv) return;
      void saveChatSession({
        sessionId: conv.id,
        title: (titleFor ?? title)(messages),
        startedAt: conv.startedAt,
        messages,
      })
        .then(() => qc.invalidateQueries({ queryKey: ["chatSessions"] }))
        .catch(() => {});
    },
    [qc, titleFor],
  );

  /**
   * Record that a proposed workout is now a real one on the Garmin account.
   *
   * Written back into the transcript and persisted, so reopening this
   * conversation shows the card as already sent rather than offering the button
   * a second time and quietly creating a duplicate.
   */
  const markSaved = useCallback(
    (messageIndex: number, draftIndex: number, workoutId: number) => {
      setHistory((prev) => {
        const next = prev.map((m, i) =>
          i !== messageIndex || !m.drafts
            ? m
            : {
                ...m,
                drafts: m.drafts.map((d, j) =>
                  j === draftIndex ? { ...d, savedWorkoutId: workoutId } : d,
                ),
              },
        );
        persist(next);
        return next;
      });
    },
    [persist],
  );

  const stop = useCallback(() => {
    const id = turn.current;
    if (id) void chatCancel(id).catch(() => {});
  }, []);

  /**
   * Answer a question the model asked. The turn is parked until this lands.
   *
   * The card locks either way: `false` means the turn stopped or the question
   * timed out while the button was going down, and a card that stays live after
   * that is a button that does nothing.
   */
  const answer = useCallback(
    async (callId: string, answers: string[]) => {
      const id = turn.current;
      if (!id) return;
      setAsks(asks.current.map((a) => (a.callId === callId ? { ...a, answers } : a)));
      await chatAnswer(id, callId, answers).catch(() => false);
    },
    [setAsks],
  );

  const send = useCallback(
    async (text: string) => {
      const question = text.trim();
      if (!question || busy) return;

      const next: ChatMessage[] = [...history, { role: "user", content: question }];
      setHistory(next);
      clearTurn();
      setPending("");
      setBusy(true);

      if (!session.current) {
        session.current = { id: rid(), startedAt: new Date().toISOString() };
        setSessionId(session.current.id);
      }
      const conv = session.current;

      // One channel per turn, so a slow previous turn can't write into this one.
      const id = rid();
      turn.current = id;
      let answered = "";
      const sources: string[] = [];
      /**
       * Each call's label and how it ended, by id.
       *
       * The saved blocks carry the label rather than an id, because nothing in a
       * landed message has ids in it — `sources` is deduplicated for the summary
       * line and a turn that read the same thing twice has fewer of those than
       * it made calls. Written twice per call; the second write is the one that
       * knows whether it worked.
       */
      const labels = new Map<string, { label: string; ok: boolean }>();
      // Held here as well as in state: the handler below and the history written
      // when the turn ends both need the current list, and a state variable read
      // from this closure would be the one captured when the turn started.

      const unlisten = await listen<ChatEvent>(`chat:${id}`, (e) => {
        const ev = e.payload;
        if (ev.type === "delta") {
          answered += ev.text;
          setPending(answered);
          pushText(ev.text);
        } else if (ev.type === "tool") {
          // The first of the pair, which is always the one that opens the row.
          // The second only changes a row that is already on screen and in the
          // order, and appending on it would draw every call twice.
          if (ev.running) pushBlock({ kind: "tool", callId: ev.callId });
          labels.set(ev.callId, { label: ev.label, ok: ev.ok });
          setSteps((prev) => {
            const at = prev.findIndex((s) => s.callId === ev.callId);
            if (at < 0) {
              return [
                ...prev,
                {
                  callId: ev.callId,
                  label: ev.label,
                  running: ev.running,
                  ok: ev.ok,
                  startedAt: Date.now(),
                },
              ];
            }
            return prev.map((s, i) =>
              i === at ? { ...s, running: ev.running, ok: ev.ok, endedAt: Date.now() } : s,
            );
          });
        } else if (ev.type === "draft") {
          // Straight onto the screen, under the answer that is still arriving.
          pushBlock({ kind: "draft", index: drafts.current.length });
          setDrafts([...drafts.current, ev.draft]);
        } else if (ev.type === "ask") {
          pushBlock({ kind: "ask", callId: ev.callId });
          setAsks([
            ...asks.current,
            {
              callId: ev.callId,
              header: ev.header ?? undefined,
              question: ev.question,
              options: ev.options,
              multi: ev.multi,
              answers: null,
            },
          ]);
        } else if (ev.type === "done") {
          sources.push(...ev.sources);
        } else if (ev.type === "error") {
          setError(ev.text);
        }
      });

      try {
        await chatSend(id, next, activityId);
        // Whatever was asked and answered while the turn ran. An unanswered one
        // is kept with an empty list — the transcript should show that the
        // question was put and went by, not silently drop it.
        const asked: AskRecord[] = asks.current.map((a) => ({
          header: a.header,
          question: a.question,
          options: a.options,
          multi: a.multi,
          answers: a.answers ?? [],
        }));
        const saved = persistedBlocks(timeline.current, labels, asks.current);
        // Rebuilt from the blocks rather than taken from `answered`, which is
        // every token in arrival order with nothing between them: a turn that
        // says "let me look" and then writes its answer would otherwise be
        // saved with the two run together into one sentence.
        const content = saved.flatMap((b) => (b.kind === "text" ? [b.text] : [])).join("\n\n");
        if (content) {
          const done: ChatMessage[] = [
            ...next,
            {
              role: "assistant",
              content,
              sources,
              blocks: saved,
              ...(drafts.current.length > 0 && { drafts: drafts.current }),
              ...(asked.length > 0 && { asks: asked }),
            },
          ];
          setHistory(done);
          // Persisting and proposing what to ask next are both nice-to-haves;
          // neither should be able to take down a turn that already succeeded.
          persist(done);
          if (wantFollowups) {
            void chatFollowups(done)
              .then((f) => {
                // Only if you haven't already moved on to another question.
                if (session.current?.id === conv.id) setFollowups(f);
              })
              .catch(() => {});
          }
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        unlisten();
        turn.current = null;
        setPending(null);
        setBusy(false);
        // These live in history from here; leaving them here as well would
        // render each one twice.
        timeline.current = [];
        setBlocks([]);
        setSteps([]);
        setDrafts([]);
        setAsks([]);
      }
    },
    [activityId, busy, clearTurn, history, persist, pushBlock, pushText, setAsks, wantFollowups],
  );

  // A turn outlives the screen that started it — the Rust side keeps streaming
  // into a channel nobody is listening to, and on a hosted provider that is real
  // money spent on an answer nobody will read.
  useEffect(
    () => () => {
      if (turn.current) void chatCancel(turn.current).catch(() => {});
    },
    [],
  );

  return {
    history,
    pending,
    blocks,
    steps,
    drafting,
    asking,
    error,
    busy,
    followups,
    sessionId,
    send,
    stop,
    answer,
    reset,
    load,
    markSaved,
    markDrafting,
  };
}
