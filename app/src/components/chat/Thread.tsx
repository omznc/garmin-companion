/**
 * The conversation, oldest first.
 *
 * It used to run newest-first, which followed from the composer being at the
 * top: the answer you just asked for had to be the first thing under it. With
 * the box at the bottom that reverses — and the reversal is the point, because a
 * conversation you can read downwards is one where the model referring to "the
 * run I mentioned above" means something.
 *
 * Nothing here is a bubble except what you said. The answer is prose at the
 * column's full measure, the same as every other piece of writing in this app;
 * putting it in a rounded rectangle with an avatar would make it look like a
 * product that has an assistant rather than an app that has a coach.
 */
import type { ChatMessage, WorkoutDraft } from "../../lib/api";
import type { PendingAsk, ToolStep } from "../../lib/useChat";
import { AiMark } from "../AiMark";
import { Markdown } from "../Markdown";
import { WorkoutCard } from "../WorkoutCard";
import { AskCard } from "./AskCard";
import { Thinking, ToolSummary, ToolTimeline } from "./ToolTimeline";

export function Thread({
  history,
  pending,
  steps,
  drafting,
  asking,
  onAnswer,
  onSaved,
  onDraftSaved,
  /** The activity page's copy, which sits under a screen that is already full. */
  compact = false,
}: {
  history: ChatMessage[];
  pending: string | null;
  steps: ToolStep[];
  drafting: WorkoutDraft[];
  asking: Array<PendingAsk & { answers: string[] | null }>;
  onAnswer: (callId: string, answers: string[]) => void;
  onSaved: (messageIndex: number, draftIndex: number, workoutId: number) => void;
  onDraftSaved: (draftIndex: number, workoutId: number) => void;
  compact?: boolean;
}) {
  const prose = compact
    ? { fontSize: "var(--fs-md)", maxWidth: "68ch" }
    : { fontSize: "var(--fs-lg)", maxWidth: "72ch" };

  return (
    // The inline copy has no docked composer to leave room for, and `--composer-h`
    // is whatever the Ask screen last measured — a page of empty space under a
    // two-line answer.
    <div className={compact ? "chat-thread chat-thread-inline" : "chat-thread"}>
      {history.map((m, i) =>
        m.role === "user" ? (
          // Index as key, deliberately: the transcript is append-only within a
          // conversation, and two identical questions are two entries.
          <div key={i} className="chat-turn">
            <div className="bubble-user">{m.content}</div>
          </div>
        ) : (
          <div key={i} className="chat-turn">
            {m.sources && m.sources.length > 0 && <ToolSummary sources={m.sources} />}
            {m.asks?.map((a, j) => (
              <AskCard
                key={j}
                header={a.header}
                question={a.question}
                options={a.options}
                multi={a.multi}
                answers={a.answers}
              />
            ))}
            <Answer text={m.content} prose={prose} />
            {m.drafts?.map((d, j) => (
              <WorkoutCard key={j} draft={d} onSaved={(id) => onSaved(i, j, id)} />
            ))}
          </div>
        ),
      )}

      {/* The turn in flight. Its question is already the last thing in history,
          so this is only ever the reply half. */}
      {pending !== null && (
        <div className="chat-turn">
          <ToolTimeline steps={steps} />
          {/* Only when nothing else in this block is saying anything. A running
              tool row already describes the wait better than this can, an
              unanswered question is waiting on you rather than on the model, and
              once prose is arriving the caret at the end of it is the signal. */}
          {!pending && !steps.some((s) => s.running) && !asking.some((a) => a.answers === null) && (
            <Thinking />
          )}
          {asking.map((a) => (
            <AskCard
              key={a.callId}
              header={a.header}
              question={a.question}
              options={a.options}
              multi={a.multi}
              answers={a.answers}
              onAnswer={a.answers === null ? (out) => onAnswer(a.callId, out) : undefined}
            />
          ))}
          {pending && <Answer text={pending} prose={prose} streaming />}
          {/* Sent from here, the id goes back into the turn's own copy of the
              draft — the transcript has no message to write it into yet, and
              the turn lands seconds later and redraws the card from history. */}
          {drafting.map((d, j) => (
            <WorkoutCard key={j} draft={d} onSaved={(id) => onDraftSaved(j, id)} />
          ))}
        </div>
      )}
    </div>
  );
}

function Answer({
  text,
  prose,
  streaming = false,
}: {
  text: string;
  prose: { fontSize: string; maxWidth: string };
  streaming?: boolean;
}) {
  return (
    <AiMark label="chat answer">
      {/* `selectable` because the answer is the one thing here worth copying,
          and the app otherwise suppresses selection. */}
      <div
        className="md-body selectable"
        style={{ ...prose, lineHeight: 1.75, textWrap: "pretty" }}
        aria-busy={streaming || undefined}
      >
        <Markdown>{text}</Markdown>
        {streaming && <span className="caret" aria-hidden />}
      </div>
    </AiMark>
  );
}
