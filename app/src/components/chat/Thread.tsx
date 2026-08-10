/**
 * The conversation, oldest first.
 *
 * It used to run newest-first, which followed from the composer being at the
 * top: the answer you just asked for had to be the first thing under it. With
 * the box at the bottom that reverses — and the reversal is the point, because a
 * conversation you can read downwards is one where the model referring to "the
 * run I mentioned above" means something.
 *
 * Downwards within a turn, too. A turn is not a block of prose with a list of
 * tool calls stacked above it: it says something, goes and reads two tables,
 * says more, asks you how long you have, and finishes. That order is what
 * `ChatMessage.blocks` records and what this draws, live and afterwards, so the
 * thread reads as the sequence it was rather than as a summary of it.
 *
 * Nothing here is a bubble except what you said. The answer is prose at the
 * column's full measure, the same as every other piece of writing in this app;
 * putting it in a rounded rectangle with an avatar would make it look like a
 * product that has an assistant rather than an app that has a coach.
 */
import type { ChatMessage, TurnBlock, WorkoutDraft } from "../../lib/api";
import type { LiveBlock, PendingAsk, ToolStep } from "../../lib/useChat";
import { AiMark } from "../AiMark";
import { Markdown } from "../Markdown";
import { WorkoutCard } from "../WorkoutCard";
import { AskCard } from "./AskCard";
import { DoneStep, Thinking, ToolSummary, ToolTimeline } from "./ToolTimeline";

type Prose = { fontSize: string; maxWidth: string };

/**
 * Consecutive blocks of one kind, gathered.
 *
 * Only tool rows need this — a round can return four calls at once, and four
 * separate row containers would space them like four separate thoughts when
 * they are one trip to the cache. Everything else passes through as a run of
 * length one.
 */
function runs<T extends { kind: string }>(blocks: T[]): T[][] {
  const out: T[][] = [];
  for (const b of blocks) {
    const last = out[out.length - 1];
    if (last && last[0].kind === "tool" && b.kind === "tool") last.push(b);
    else out.push([b]);
  }
  return out;
}

export function Thread({
  history,
  pending,
  blocks,
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
  blocks: LiveBlock[];
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
            <Landed
              message={m}
              prose={prose}
              onSaved={(draftIndex, workoutId) => onSaved(i, draftIndex, workoutId)}
            />
          </div>
        ),
      )}

      {/* The turn in flight. Its question is already the last thing in history,
          so this is only ever the reply half. */}
      {pending !== null && (
        <div className="chat-turn">
          <Live
            blocks={blocks}
            steps={steps}
            asking={asking}
            drafting={drafting}
            prose={prose}
            onAnswer={onAnswer}
            onDraftSaved={onDraftSaved}
          />
        </div>
      )}
    </div>
  );
}

/**
 * A turn that has landed, in the order it happened.
 *
 * The tool rows stay where they were made rather than collapsing into a line at
 * the top. That collapse existed because every row in a turn used to stack above
 * the answer, and eight of them there is a filing cabinet in front of the thing
 * you asked for — but in position they are two or three at a time, between
 * paragraphs, which is where they read as provenance rather than as an index.
 *
 * `blocks` is missing on anything written before it existed, and those messages
 * keep exactly the shape they were saved in: one summary line, then the
 * questions, then the answer. Rewriting them into an order nobody recorded would
 * be inventing one.
 */
function Landed({
  message,
  prose,
  onSaved,
}: {
  message: ChatMessage;
  prose: Prose;
  onSaved: (draftIndex: number, workoutId: number) => void;
}) {
  if (!message.blocks || message.blocks.length === 0) {
    return (
      <>
        {message.sources && message.sources.length > 0 && <ToolSummary sources={message.sources} />}
        {message.asks?.map((a, j) => (
          <AskCard
            key={j}
            header={a.header}
            question={a.question}
            options={a.options}
            multi={a.multi}
            answers={a.answers}
          />
        ))}
        <Answer text={message.content} prose={prose} />
        {message.drafts?.map((d, j) => (
          <WorkoutCard key={j} draft={d} onSaved={(id) => onSaved(j, id)} />
        ))}
      </>
    );
  }

  return (
    <>
      {runs(message.blocks).map((run, i) => (
        <Run key={i} blocks={run} message={message} prose={prose} onSaved={onSaved} />
      ))}
    </>
  );
}

function Run({
  blocks,
  message,
  prose,
  onSaved,
}: {
  blocks: TurnBlock[];
  message: ChatMessage;
  prose: Prose;
  onSaved: (draftIndex: number, workoutId: number) => void;
}) {
  if (blocks[0].kind === "tool") {
    // `data-landed` turns off the row's entrance: a call sliding in is for one
    // arriving live, and the moment a turn lands its rows are redrawn from the
    // message — replaying it there animates the handover rather than an event.
    return (
      <div className="tool-steps" data-landed>
        {blocks.map((b, i) =>
          b.kind === "tool" ? <DoneStep key={i} label={b.label} ok={b.ok} /> : null,
        )}
      </div>
    );
  }

  const block = blocks[0];
  if (block.kind === "text") return <Answer text={block.text} prose={prose} />;
  if (block.kind === "ask") {
    const ask = message.asks?.[block.index];
    return ask ? (
      <AskCard
        header={ask.header}
        question={ask.question}
        options={ask.options}
        multi={ask.multi}
        answers={ask.answers}
      />
    ) : null;
  }
  const draft = message.drafts?.[block.index];
  return draft ? <WorkoutCard draft={draft} onSaved={(id) => onSaved(block.index, id)} /> : null;
}

/**
 * The turn still running, drawn from the same order as a landed one.
 *
 * The blocks say where each thing goes; `steps`, `asking` and `drafting` say
 * what it currently is, because all three are still moving — a row has a clock
 * on it, a question is waiting on a button, and a card can be sent to Garmin
 * before the turn it belongs to has finished.
 */
function Live({
  blocks,
  steps,
  asking,
  drafting,
  prose,
  onAnswer,
  onDraftSaved,
}: {
  blocks: LiveBlock[];
  steps: ToolStep[];
  asking: Array<PendingAsk & { answers: string[] | null }>;
  drafting: WorkoutDraft[];
  prose: Prose;
  onAnswer: (callId: string, answers: string[]) => void;
  onDraftSaved: (draftIndex: number, workoutId: number) => void;
}) {
  const last = blocks[blocks.length - 1];
  /**
   * Nothing on screen is saying anything, so the turn has to.
   *
   * Three waits look like this: before the first tool call, while the model
   * decides what to read; after the last one lands, while it writes; and the one
   * between a paragraph ending and the next call being made. A running row
   * describes its own wait better than this can, an unanswered question is
   * waiting on you rather than on the model, and prose arriving has a caret on
   * the end of it — so this is only for when there is none of that.
   */
  const quiet =
    last?.kind !== "text" &&
    !steps.some((s) => s.running) &&
    !asking.some((a) => a.answers === null);

  return (
    <>
      {runs(blocks).map((run, i) => {
        if (run[0].kind === "tool") {
          const live = run.flatMap((b) =>
            b.kind === "tool" ? steps.filter((s) => s.callId === b.callId) : [],
          );
          return <ToolTimeline key={i} steps={live} />;
        }
        const block = run[0];
        if (block.kind === "text") {
          // The caret goes on the paragraph still being written, which is the
          // last block and only while it is last — once a tool call lands under
          // it, that paragraph is finished.
          return <Answer key={i} text={block.text} prose={prose} streaming={block === last} />;
        }
        if (block.kind === "ask") {
          const ask = asking.find((a) => a.callId === block.callId);
          return ask ? (
            <AskCard
              key={i}
              header={ask.header}
              question={ask.question}
              options={ask.options}
              multi={ask.multi}
              answers={ask.answers}
              onAnswer={ask.answers === null ? (out) => onAnswer(ask.callId, out) : undefined}
            />
          ) : null;
        }
        const draft = drafting[block.index];
        // Sent from here, the id goes back into the turn's own copy of the draft
        // — the transcript has no message to write it into yet, and the turn
        // lands seconds later and redraws the card from history.
        return draft ? (
          <WorkoutCard key={i} draft={draft} onSaved={(id) => onDraftSaved(block.index, id)} />
        ) : null;
      })}
      {quiet && <Thinking />}
    </>
  );
}

function Answer({
  text,
  prose,
  streaming = false,
}: {
  text: string;
  prose: Prose;
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
