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
 * Conversations are saved the same way the Ask screen saves them, so one started
 * here shows up in the history there. It runs on the same `useChat` too, which
 * is how it inherited drafted workouts, questions the model asks back and the
 * tool timeline without any of them being written twice.
 *
 * What it does not inherit is the docked composer. This is a footnote at the
 * bottom of a page that already has the numbers on it, and a box fixed over that
 * page would claim the screen for a conversation you may not be having. The box
 * stays in the flow, under the transcript, where the rest of the page's controls
 * are.
 */
import { useState } from "react";
import { Link } from "@tanstack/react-router";
import { useQuery } from "@tanstack/react-query";
import { chatConfig, type ChatMessage } from "../lib/api";
import { useChat } from "../lib/useChat";
import { Thread } from "./chat/Thread";
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

export function ActivityChat({
  activityId,
  activityName,
}: {
  activityId: number;
  activityName: string;
}) {
  const config = useQuery({ queryKey: ["chatConfig"], queryFn: chatConfig });
  const [draft, setDraft] = useState("");

  const chat = useChat({
    activityId,
    // Titled by the session rather than by the question, so the Ask screen's
    // history says which run a conversation was about.
    titleFor: (messages: ChatMessage[]) =>
      `${activityName} — ${messages.find((m) => m.role === "user")?.content ?? "Untitled"}`,
    // The Ask screen has a row to put them in. This doesn't, and a request per
    // answer for suggestions nothing renders is a request for nothing.
    followups: false,
  });

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

  const empty = chat.history.length === 0 && chat.pending === null;
  const blocked = chat.asking.some((a) => a.answers === null);

  function send(text: string) {
    setDraft("");
    void chat.send(text);
  }

  return (
    <div>
      {!empty && (
        <Thread
          history={chat.history}
          pending={chat.pending}
          blocks={chat.blocks}
          steps={chat.steps}
          drafting={chat.drafting}
          asking={chat.asking}
          onAnswer={(callId, answers) => void chat.answer(callId, answers)}
          onSaved={chat.markSaved}
          onDraftSaved={chat.markDrafting}
          compact
        />
      )}

      <div className="composer-box" style={{ marginTop: empty ? 0 : 8 }}>
        <textarea
          rows={1}
          value={draft}
          placeholder={
            blocked ? "Answer the question above to carry on…" : "Ask about this session…"
          }
          disabled={blocked}
          aria-label="Ask about this session"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.shiftKey) return;
            if (e.nativeEvent.isComposing) return;
            e.preventDefault();
            if (draft.trim() && !chat.busy && !blocked) send(draft);
          }}
        />
        <button
          type="button"
          className="composer-send"
          data-stop={chat.busy || undefined}
          disabled={!chat.busy && (!draft.trim() || blocked)}
          aria-label={chat.busy ? "Stop" : "Send"}
          onClick={() => (chat.busy ? chat.stop() : send(draft))}
        >
          {chat.busy ? (
            <svg width="10" height="10" viewBox="0 0 11 11" aria-hidden>
              <rect width="11" height="11" rx="2" fill="currentColor" />
            </svg>
          ) : (
            <SendIcon size={15} aria-hidden />
          )}
        </button>
      </div>

      {empty && !chat.busy && (
        <div className="chip-row" style={{ margin: "12px 0 0" }}>
          {OPENERS.slice(0, SUGGESTED).map((q, i) => (
            <button
              key={q}
              type="button"
              className="chip"
              style={{ animationDelay: `${i * 35}ms` }}
              onClick={() => send(q)}
            >
              {q}
            </button>
          ))}
        </div>
      )}

      {chat.error && (
        <div style={{ fontSize: "var(--fs-small)", color: "var(--warn)", marginTop: 16 }}>
          {chat.error}
        </div>
      )}
    </div>
  );
}
