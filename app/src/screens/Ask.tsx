import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  chatConfig,
  chatFollowups,
  chatSend,
  chatSession,
  chatSessions,
  deleteChatSession,
  saveChatSession,
  type ChatMessage,
  type ChatSessionMeta,
  type WorkoutDraft,
} from "../lib/api";
import { Empty, ErrorNote, Loading, PageHeader } from "../components/ui";
import { DeleteIcon, NewIcon, PinIcon, SendIcon, UnpinIcon } from "../lib/icons";
import { Markdown } from "../components/Markdown";
import { ToolLoader } from "../components/ToolLoader";
import { WorkoutCard } from "../components/WorkoutCard";
import { since } from "../lib/format";

/**
 * Openers for a conversation that hasn't started, drawn from before the model
 * has anything to go on. It's a pool rather than a list because three fixed
 * questions in a fixed order stop being suggestions after the second visit —
 * `sample` takes three per visit, so the row proposes something you didn't ask
 * last time. Once an answer exists the model's own follow-ups take the slot.
 */
const OPENERS = [
  "Am I recovered enough to go hard today?",
  "How much of my last five runs was above Z2?",
  "Compare my last three runs.",
  "Is my cadence improving?",
  "Am I drifting back into Z5?",
  "How long was my longest easy run this month?",
  "What does my HRV trend say about this week?",
  "Is my resting heart rate moving?",
  "How did I sleep before my best run?",
  "What should this week's long run look like?",
  "Am I running more than last month?",
  "Which run this month was best executed?",
  "How much of my week was easy versus hard?",
  "What's holding my VO2 max back?",
];

/** Suggested slots. Pinned questions get their own, up to `MAX_PINS`. */
const SUGGESTED = 3;
const MAX_PINS = 6;

/** Pinned questions, per machine — they're a personal shortlist, not app data. */
const PINS_KEY = "garmin-companion:ask-pins";

/** How many past conversations each scroll fetches. */
const PAGE = 15;

/**
 * The pinned shortlist.
 *
 * Read once on mount and written through on every change: the list is short,
 * only this screen touches it, and losing it to a failed write would be worse
 * than the write costing a millisecond.
 */
function usePins() {
  const [pins, setPins] = useState<string[]>(() => {
    try {
      const raw = JSON.parse(localStorage.getItem(PINS_KEY) ?? "[]");
      if (!Array.isArray(raw)) return [];
      return raw.filter((x): x is string => typeof x === "string").slice(0, MAX_PINS);
    } catch {
      return [];
    }
  });

  function write(next: string[]) {
    setPins(next);
    try {
      localStorage.setItem(PINS_KEY, JSON.stringify(next));
    } catch {
      // A full or blocked store costs the pins on next launch, nothing here.
    }
  }

  return {
    pins,
    full: pins.length >= MAX_PINS,
    toggle: (q: string) => {
      if (pins.includes(q)) write(pins.filter((p) => p !== q));
      else if (pins.length < MAX_PINS) write([...pins, q]);
    },
  };
}

/** `n` distinct items, in random order. Used for the openers, so unseeded. */
function sample<T>(xs: readonly T[], n: number): T[] {
  const pool = [...xs];
  for (let i = pool.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [pool[i], pool[j]] = [pool[j], pool[i]];
  }
  return pool.slice(0, n);
}

type ChatEvent =
  | { type: "status"; text: string }
  | { type: "delta"; text: string }
  | { type: "draft"; draft: WorkoutDraft }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string };

export function Ask() {
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  const qc = useQueryClient();

  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [followups, setFollowups] = useState<string[]>([]);
  /** Workouts drafted during the turn being answered, before it's in history. */
  const [drafting, setDrafting] = useState<WorkoutDraft[]>([]);
  const top = useRef<HTMLDivElement>(null);
  const { pins, full, toggle } = usePins();
  // Drawn once per visit rather than per render, or every keystroke would
  // reshuffle the row underneath the pointer.
  const [openers] = useState(() => sample(OPENERS, SUGGESTED + MAX_PINS));

  /* The conversation being written to. Created lazily on the first question so
   * that opening the screen and leaving doesn't litter the cache with empties. */
  const session = useRef<{ id: string; startedAt: string } | null>(null);

  // Newest first means the thing you just asked is already at the top of the
  // transcript — so the scroll goes up to meet it, not down.
  useEffect(() => {
    if (history.length > 0) top.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  }, [history.length]);

  const reset = useCallback(() => {
    session.current = null;
    setHistory([]);
    setPending(null);
    setStatus(null);
    setError(null);
    setFollowups([]);
    setDrafting([]);
  }, []);

  /** Reopen an earlier conversation and keep writing into it. */
  const load = useCallback(async (id: string) => {
    const s = await chatSession(id);
    if (!s) return;
    session.current = { id: s.sessionId, startedAt: s.startedAt };
    setHistory(JSON.parse(s.messages) as ChatMessage[]);
    setPending(null);
    setStatus(null);
    setError(null);
    setFollowups([]);
    setDrafting([]);
  }, []);

  /**
   * Record that a proposed workout is now a real one on the Garmin account.
   *
   * Written back into the transcript and persisted, so reopening this
   * conversation shows the card as already sent rather than offering the
   * button a second time and quietly creating a duplicate.
   */
  const markSaved = useCallback((messageIndex: number, draftIndex: number, workoutId: number) => {
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
      const conv = session.current;
      if (conv) {
        void saveChatSession({
          sessionId: conv.id,
          title: title(next),
          startedAt: conv.startedAt,
          messages: next,
        }).catch(() => {});
      }
      return next;
    });
  }, []);

  if (config.isLoading) return <Loading label="Checking your model settings" />;

  const ready = config.data?.provider && config.data.model;

  async function send(text: string) {
    const question = text.trim();
    if (!question || busy) return;

    const next: ChatMessage[] = [...history, { role: "user", content: question }];
    setHistory(next);
    setDraft("");
    setError(null);
    setStatus(null);
    setFollowups([]);
    setDrafting([]);
    setPending("");
    setBusy(true);

    if (!session.current) {
      session.current = {
        id: `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        startedAt: new Date().toISOString(),
      };
    }
    const conv = session.current;

    // One channel per turn, so a slow previous turn can't write into this one.
    const id = `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    let answer = "";
    const sources: string[] = [];
    // Held here as well as in state: the handler below and the history written
    // when the turn ends both need the current list, and a state variable read
    // from this closure would be the one captured when the turn started.
    const drafted: WorkoutDraft[] = [];

    const unlisten = await listen<ChatEvent>(`chat:${id}`, (e) => {
      const ev = e.payload;
      if (ev.type === "delta") {
        answer += ev.text;
        setPending(answer);
        setStatus(null);
      } else if (ev.type === "status") {
        setStatus(ev.text);
      } else if (ev.type === "draft") {
        // Straight onto the screen, under the answer that is still arriving.
        drafted.push(ev.draft);
        setDrafting([...drafted]);
      } else if (ev.type === "done") {
        sources.push(...ev.sources);
      } else if (ev.type === "error") {
        setError(ev.text);
      }
    });

    try {
      await chatSend(id, next);
      if (answer.trim()) {
        const done: ChatMessage[] = [
          ...next,
          {
            role: "assistant",
            content: answer,
            sources,
            ...(drafted.length > 0 && { drafts: drafted }),
          },
        ];
        setHistory(done);
        // Persisting and proposing what to ask next are both nice-to-haves;
        // neither should be able to take down a turn that already succeeded.
        void saveChatSession({
          sessionId: conv.id,
          title: title(done),
          startedAt: conv.startedAt,
          messages: done,
        })
          .then(() => qc.invalidateQueries({ queryKey: ["chatSessions"] }))
          .catch(() => {});
        void chatFollowups(done)
          .then((f) => {
            // Only if you haven't already moved on to another question.
            if (session.current?.id === conv.id) setFollowups(f);
          })
          .catch(() => {});
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      unlisten();
      setPending(null);
      setStatus(null);
      setBusy(false);
      // The cards live in history from here; leaving them here as well would
      // render each one twice.
      setDrafting([]);
    }
  }

  // The model's own suggestions when it produced any, this visit's openers when
  // it didn't — the row being empty half the time was worse than it being
  // generic. Pinned questions come first and keep their place; the suggested
  // three fill in behind them, minus anything already pinned so the row never
  // offers the same question twice.
  const suggested = (followups.length > 0 ? followups : openers)
    .filter((q) => !pins.includes(q))
    .slice(0, SUGGESTED);

  return (
    <div className="screen">
      <PageHeader
        eyebrow={ready ? config.data!.model : "No model configured"}
        title="Ask"
        lede="Reading your cached activities, zones, cadence and recovery. Only the metrics a question needs are sent."
        space={30}
      />

      {!ready ? (
        <Empty
          title="Choose a model first."
          body="Answers come from a model you point this at — the built-in coach, OpenRouter with your own key, or a local Ollama. None is configured yet, and nothing is sent anywhere until one is."
          action={
            <Link className="cta" to="/settings">
              Open settings
            </Link>
          }
        />
      ) : (
        <>
          <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
            <input
              className="input-bare"
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && !e.shiftKey) {
                  e.preventDefault();
                  void send(draft);
                }
              }}
              placeholder="Ask about your training…"
              disabled={busy}
              style={{ flex: 1 }}
            />
            <button
              onClick={() => void send(draft)}
              disabled={busy || !draft.trim()}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 7,
                fontSize: "var(--fs-small)",
                color: busy || !draft.trim() ? "var(--faint)" : "var(--acc)",
                whiteSpace: "nowrap",
                cursor: busy || !draft.trim() ? "default" : "pointer",
              }}
            >
              {busy ? "Thinking" : "Send"}
              {!busy && <SendIcon size={14} style={{ flex: "none" }} aria-hidden />}
            </button>
          </div>
          <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 10 }}>
            {config.data!.provider === "ollama"
              ? "Sent to Ollama on this machine. Nothing leaves your computer."
              : config.data!.provider === "cloud"
                ? "The question and the metrics it needs go to this project's server, which forwards them to a model. Raw GPS never is."
                : "The question and the metrics it needs are sent to OpenRouter. Raw GPS never is."}
          </div>

          {/* Openers before you've said anything, the model's own suggestions
              after — same slot either way, with your pins ahead of both. */}
          {(pins.length > 0 || suggested.length > 0) && !busy && pending == null && (
            <div style={{ display: "flex", gap: 22, flexWrap: "wrap", marginTop: 22 }}>
              {pins.map((s) => (
                <Prompt
                  key={s}
                  text={s}
                  pinned
                  canPin
                  onSend={() => void send(s)}
                  onToggle={() => toggle(s)}
                />
              ))}
              {suggested.map((s) => (
                <Prompt
                  key={s}
                  text={s}
                  pinned={false}
                  canPin={!full}
                  onSend={() => void send(s)}
                  onToggle={() => toggle(s)}
                />
              ))}
            </div>
          )}

          {error && <ErrorNote error={error} />}

          {(history.length > 0 || pending != null) && (
            <>
              <div className="section-head" style={{ margin: "52px 0 34px" }}>
                <div className="eyebrow">This session</div>
                <button
                  className="quiet"
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 6,
                    fontSize: "var(--fs-caption)",
                  }}
                  onClick={reset}
                  disabled={busy}
                >
                  <NewIcon size={13} style={{ flex: "none" }} aria-hidden />
                  New conversation
                </button>
              </div>
              {/* Newest at the top, next to the box you type in — reading the
                  answer you just asked for should never mean scrolling past
                  every answer before it. */}
              <div ref={top} style={{ display: "flex", flexDirection: "column", gap: 44 }}>
                {turns(history, pending, drafting).map((t) => (
                  <div key={t.key} style={{ display: "flex", flexDirection: "column", gap: 20 }}>
                    <Question>{t.question}</Question>
                    {t.answer != null && (
                      <Answer
                        text={t.answer}
                        sources={t.sources}
                        drafts={t.drafts}
                        onSaved={(draftIndex, workoutId) =>
                          markSaved(t.key + 1, draftIndex, workoutId)
                        }
                        status={status}
                        streaming={t.streaming}
                      />
                    )}
                  </div>
                ))}
              </div>
            </>
          )}

          {history.length === 0 && pending == null && <Earlier onLoad={load} />}
        </>
      )}
    </div>
  );
}

/**
 * One question in the suggestion row: the question itself, and the pin toggle
 * beside it.
 *
 * The toggle only appears on hover — the row is meant to read as a line of
 * questions, and a control next to each one turns it into a list of settings.
 * A pinned question keeps its toggle reachable the same way, and shows in the
 * foreground colour so the shortlist is distinguishable from the suggestions
 * that rotate behind it.
 */
function Prompt({
  text,
  pinned,
  canPin,
  onSend,
  onToggle,
}: {
  text: string;
  pinned: boolean;
  canPin: boolean;
  onSend: () => void;
  onToggle: () => void;
}) {
  const [hover, setHover] = useState(false);

  return (
    <span
      style={{ display: "inline-flex", alignItems: "baseline", gap: 7 }}
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <button
        className="underlined"
        style={{ fontSize: "var(--fs-small)", color: pinned ? "var(--fg)" : undefined }}
        onClick={onSend}
      >
        {text}
      </button>
      {/* The slot is held open whether the icon shows or not, so the row of
          questions doesn't shuffle sideways as the pointer crosses it. */}
      <button
        className="quiet"
        onClick={onToggle}
        disabled={!canPin}
        aria-label={pinned ? "Unpin this question" : "Pin this question"}
        title={pinned ? "Unpin this question" : `Pin this question (up to ${MAX_PINS})`}
        style={{
          display: "grid",
          placeItems: "center",
          width: 14,
          color: pinned ? "var(--acc)" : "var(--faint)",
          // A pin already set stays visible: it's the mark that says why this
          // question is here and not rotating with the rest.
          visibility: pinned || (hover && canPin) ? "visible" : "hidden",
          alignSelf: "center",
        }}
      >
        {pinned ? <UnpinIcon size={13} /> : <PinIcon size={13} />}
      </button>
    </span>
  );
}

interface Turn {
  key: number;
  question: string;
  answer: string | null;
  sources?: string[];
  drafts?: WorkoutDraft[];
  streaming: boolean;
}

/**
 * Pair each question with its answer, newest first.
 *
 * The history is a flat alternating list because that's what the model is
 * sent; a question and its answer only become one thing here, where they have
 * to move up the page together.
 */
function turns(history: ChatMessage[], pending: string | null, drafting: WorkoutDraft[]): Turn[] {
  const out: Turn[] = [];
  for (let i = 0; i < history.length; i++) {
    const m = history[i];
    if (m.role !== "user") continue;
    const reply = history[i + 1]?.role === "assistant" ? history[i + 1] : null;
    out.push({
      key: i,
      question: m.content,
      answer: reply?.content ?? null,
      sources: reply?.sources,
      drafts: reply?.drafts,
      streaming: false,
    });
  }
  // The turn being answered right now: its question is already the last entry
  // in history, so this fills in the answer rather than adding a turn.
  if (pending != null && out.length > 0) {
    const last = out[out.length - 1];
    if (last.answer == null) {
      last.answer = pending;
      last.streaming = true;
      // Anything drafted so far this turn. It moves into history when the turn
      // ends, and `drafting` is cleared in the same breath.
      if (drafting.length > 0) last.drafts = drafting;
    }
  }
  return out.reverse();
}

/** What a conversation is "about": the question that started it. */
function title(messages: ChatMessage[]): string {
  const first = messages.find((m) => m.role === "user")?.content.trim() ?? "Untitled";
  return first.length > 120 ? `${first.slice(0, 119)}…` : first;
}

/**
 * Past conversations, oldest fetched a page at a time as you reach the end.
 *
 * Only rendered on an empty conversation — below a transcript it would be a
 * list of other transcripts under the one you're reading.
 */
function Earlier({ onLoad }: { onLoad: (id: string) => void }) {
  const qc = useQueryClient();
  const sentinel = useRef<HTMLDivElement>(null);

  const q = useInfiniteQuery({
    queryKey: ["chatSessions"],
    queryFn: ({ pageParam }) => chatSessions(PAGE, pageParam),
    initialPageParam: 0,
    // A short page means the end; otherwise ask for everything past what we hold.
    getNextPageParam: (last, all) =>
      last.length < PAGE ? undefined : all.reduce((n, p) => n + p.length, 0),
  });

  const { hasNextPage, isFetchingNextPage, fetchNextPage } = q;

  useEffect(() => {
    const el = sentinel.current;
    if (!el || !hasNextPage) return;
    const io = new IntersectionObserver((entries) => {
      if (entries[0].isIntersecting && !isFetchingNextPage) void fetchNextPage();
    });
    io.observe(el);
    return () => io.disconnect();
  }, [hasNextPage, isFetchingNextPage, fetchNextPage]);

  const sessions = q.data?.pages.flat() ?? [];
  if (sessions.length === 0) return null;

  async function remove(id: string) {
    await deleteChatSession(id);
    await qc.invalidateQueries({ queryKey: ["chatSessions"] });
  }

  return (
    <div style={{ marginTop: 58 }}>
      <div className="eyebrow" style={{ marginBottom: 18 }}>
        Earlier
      </div>
      {sessions.map((s) => (
        <Past key={s.sessionId} session={s} onLoad={onLoad} onDelete={remove} />
      ))}
      {/* Sits below the last row; crossing it pulls the next page in. */}
      <div ref={sentinel} style={{ height: 1 }} />
      {isFetchingNextPage && (
        <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", padding: "14px 0" }}>
          Loading…
        </div>
      )}
    </div>
  );
}

function Past({
  session,
  onLoad,
  onDelete,
}: {
  session: ChatSessionMeta;
  onLoad: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const [hover, setHover] = useState(false);

  return (
    <div
      className="row-group"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <button className="row" style={{ flex: 1 }} onClick={() => onLoad(session.sessionId)}>
        <span
          style={{
            flex: 1,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {session.title}
        </span>
        <span
          className="mono"
          style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", flex: "none" }}
        >
          {session.messageCount}
        </span>
        <span
          style={{
            fontSize: "var(--fs-caption)",
            color: "var(--faint)",
            flex: "none",
            minWidth: 96,
          }}
        >
          {since(session.updatedAt)}
        </span>
      </button>
      {/* Last in the row and vertically centred against it. The slot is always
          there — it keeps the row's right edge steady — but the icon only shows
          on hover, since deleting is never the reason you came to this list. */}
      <button
        className="quiet"
        title="Delete this conversation"
        aria-label="Delete this conversation"
        onClick={() => onDelete(session.sessionId)}
        style={{
          flex: "none",
          display: "grid",
          placeItems: "center",
          width: 24,
          height: 24,
          marginLeft: 6,
          color: "var(--faint)",
          visibility: hover ? "visible" : "hidden",
        }}
      >
        <DeleteIcon size={15} aria-hidden />
      </button>
    </div>
  );
}

function Question({ children }: { children: React.ReactNode }) {
  return (
    <div
      className="serif"
      style={{
        fontStyle: "italic",
        fontSize: 25,
        lineHeight: 1.45,
        paddingLeft: 18,
        borderLeft: "1px solid var(--line)",
      }}
    >
      {children}
    </div>
  );
}

function Answer({
  text,
  sources,
  drafts,
  onSaved,
  status,
  streaming = false,
}: {
  text: string;
  sources?: string[];
  drafts?: WorkoutDraft[];
  onSaved: (draftIndex: number, workoutId: number) => void;
  status?: string | null;
  streaming?: boolean;
}) {
  return (
    <div>
      {/* `selectable` because the answer is the one thing here worth copying,
          and the app otherwise suppresses selection. */}
      <div
        className="md-body selectable"
        style={{ fontSize: "var(--fs-lg)", lineHeight: 1.75, maxWidth: "72ch", textWrap: "pretty" }}
      >
        <Markdown>{text}</Markdown>
        {/* A caret once prose is arriving; before that, the loader — which says
            which of your data it went to read. */}
        {streaming &&
          (text ? <span className="caret" aria-hidden /> : <ToolLoader label={status} />)}
      </div>
      {/* Below the prose explaining it, and above the list of what was read —
          the workout is the thing you act on, not a footnote about sources. */}
      {drafts?.map((d, i) => (
        <WorkoutCard key={i} draft={d} onSaved={(workoutId) => onSaved(i, workoutId)} />
      ))}
      {sources && sources.length > 0 && (
        <div style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", marginTop: 12 }}>
          Read: {dedupe(sources).join(" · ")}
        </div>
      )}
    </div>
  );
}

const dedupe = (xs: string[]) => [...new Set(xs)];
