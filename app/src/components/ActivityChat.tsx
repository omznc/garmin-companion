/**
 * Asking about one session, on the session's own page.
 *
 * The same model, the same tools and the same cache as the Ask screen — the
 * only difference is that this session's analysis is put in front of the model
 * first, so "was that too hard?" has an antecedent and doesn't need a date
 * attached to it. The model still reaches past the session when the answer
 * needs the weeks around it; the context narrows what a pronoun refers to, not
 * what can be read.
 *
 * Conversations are saved the same way the Ask screen saves them, so one
 * started here shows up in the history there. The transcript is deliberately
 * plain: this is a footnote to a page that already has the numbers on it, not a
 * second chat application.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import { chatConfig, chatSend, saveChatSession, type ChatMessage } from "../lib/api";
import { Markdown } from "./Markdown";
import { ToolLoader } from "./ToolLoader";
import { SendIcon } from "../lib/icons";

/**
 * Openers, chosen for what a session page can't already answer by being read.
 * "What was my average heart rate" is on the screen; these are not.
 */
const OPENERS = [
  "Was this too hard for an easy run?",
  "What should I do differently next time?",
  "How does this compare to my last few?",
  "Am I recovered enough to go again tomorrow?",
  "What does the drift in this one tell me?",
];

/** How many openers are offered at once. */
const SUGGESTED = 2;

type ChatEvent =
  | { type: "status"; text: string }
  | { type: "delta"; text: string }
  | { type: "done"; sources: string[] }
  | { type: "error"; text: string };

export function ActivityChat({
  activityId,
  activityName,
}: {
  activityId: number;
  activityName: string;
}) {
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  const qc = useQueryClient();

  const [history, setHistory] = useState<ChatMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  /** Created lazily on the first question, so opening a page saves nothing. */
  const session = useRef<{ id: string; startedAt: string } | null>(null);

  // Opening a different activity is a different conversation. Without this,
  // navigating between two runs would carry the first one's transcript onto the
  // second one's page, under the second one's numbers.
  useEffect(() => {
    session.current = null;
    setHistory([]);
    setPending(null);
    setStatus(null);
    setError(null);
    setDraft("");
  }, [activityId]);

  const send = useCallback(
    async (text: string) => {
      const question = text.trim();
      if (!question || busy) return;

      const next: ChatMessage[] = [...history, { role: "user", content: question }];
      setHistory(next);
      setDraft("");
      setError(null);
      setStatus(null);
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

      const unlisten = await listen<ChatEvent>(`chat:${id}`, (e) => {
        const ev = e.payload;
        if (ev.type === "delta") {
          answer += ev.text;
          setPending(answer);
          setStatus(null);
        } else if (ev.type === "status") {
          setStatus(ev.text);
        } else if (ev.type === "done") {
          sources.push(...ev.sources);
        } else if (ev.type === "error") {
          setError(ev.text);
        }
      });

      try {
        await chatSend(id, next, activityId);
        if (answer.trim()) {
          const done: ChatMessage[] = [...next, { role: "assistant", content: answer, sources }];
          setHistory(done);
          // Titled by the session rather than by the question, so the Ask
          // screen's history says which run a conversation was about.
          void saveChatSession({
            sessionId: conv.id,
            title: `${activityName} — ${next[0].content}`,
            startedAt: conv.startedAt,
            messages: done,
          })
            .then(() => qc.invalidateQueries({ queryKey: ["chatSessions"] }))
            .catch(() => {});
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        unlisten();
        setPending(null);
        setStatus(null);
        setBusy(false);
      }
    },
    [activityId, activityName, busy, history, qc],
  );

  if (config.isLoading) return null;

  const ready = config.data?.provider && config.data.model;
  if (!ready) {
    // Not an `Empty` — this is the bottom of a page that worked, and a full
    // empty state here would read as the page having failed.
    return (
      <div style={{ fontSize: "var(--fs-small)", color: "var(--faint)", maxWidth: "58ch" }}>
        <Link className="underlined" to="/settings">
          Choose a model
        </Link>{" "}
        to ask about this session.
      </div>
    );
  }

  const turns: Array<{ question: string; answer: string | null; streaming: boolean }> = [];
  for (let i = 0; i < history.length; i++) {
    if (history[i].role !== "user") continue;
    const reply = history[i + 1]?.role === "assistant" ? history[i + 1] : null;
    turns.push({ question: history[i].content, answer: reply?.content ?? null, streaming: false });
  }
  if (pending != null && turns.length > 0) {
    const last = turns[turns.length - 1];
    if (last.answer == null) {
      last.answer = pending;
      last.streaming = true;
    }
  }

  return (
    <div>
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
          placeholder="Ask about this session…"
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

      {turns.length === 0 && !busy && (
        <div style={{ display: "flex", gap: 22, flexWrap: "wrap", marginTop: 18 }}>
          {OPENERS.slice(0, SUGGESTED).map((q) => (
            <button
              key={q}
              className="underlined"
              style={{ fontSize: "var(--fs-small)" }}
              onClick={() => void send(q)}
            >
              {q}
            </button>
          ))}
        </div>
      )}

      {error && (
        <div style={{ fontSize: "var(--fs-small)", color: "var(--warn)", marginTop: 16 }}>
          {error}
        </div>
      )}

      {turns.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 30, marginTop: 32 }}>
          {turns.map((t, i) => (
            <div key={i} style={{ display: "flex", flexDirection: "column", gap: 14 }}>
              <div
                className="serif"
                style={{
                  fontStyle: "italic",
                  fontSize: 20,
                  lineHeight: 1.45,
                  paddingLeft: 16,
                  borderLeft: "1px solid var(--line)",
                }}
              >
                {t.question}
              </div>
              {t.answer != null && (
                <div
                  className="md-body selectable"
                  style={{
                    fontSize: "var(--fs-md)",
                    lineHeight: 1.75,
                    maxWidth: "68ch",
                    textWrap: "pretty",
                  }}
                >
                  <Markdown>{t.answer}</Markdown>
                  {t.streaming &&
                    (t.answer ? (
                      <span className="caret" aria-hidden />
                    ) : (
                      <ToolLoader label={status} />
                    ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
