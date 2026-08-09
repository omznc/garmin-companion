/**
 * What the model went and read, while it is reading it.
 *
 * The version this replaces was a single line — "Reading your last 10
 * activities…" — overwritten by each new tool and gone the moment the first
 * word of prose arrived. Which meant the most interesting thing this app does,
 * going and reading your own data before it says anything, was visible for
 * about two seconds per question and never afterwards.
 *
 * So: one row per call, in the order they were made, kept for the length of the
 * turn. A row is running or it isn't, and the mark on the left says which. When
 * the turn lands the whole block collapses into one line on the message, which
 * is [`ToolSummary`] below — still there, no longer the first thing on the page.
 */
import { useEffect, useState } from "react";
import { DoneIcon } from "../../lib/icons";
import type { ToolStep } from "../../lib/useChat";

/** Seconds, ticking, for the row that is taking a while. */
function useElapsed(since: number, live: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!live) return;
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [live]);
  return Math.floor(((live ? now : since) - since) / 1000);
}

export function ToolTimeline({ steps }: { steps: ToolStep[] }) {
  if (steps.length === 0) return null;
  return (
    <div className="tool-steps">
      {steps.map((s) => (
        <Step key={s.callId} step={s} />
      ))}
    </div>
  );
}

function Step({ step }: { step: ToolStep }) {
  const elapsed = useElapsed(step.startedAt, step.running);
  const took = step.endedAt ? Math.round((step.endedAt - step.startedAt) / 100) / 10 : null;

  return (
    <div className="tool-step" data-running={step.running}>
      <span className="tool-step-mark" aria-hidden>
        {/* One mark or the other, never both. This used to draw the pair stacked
            and crossfade their opacity, to keep the row's text from shifting by
            a pixel as a call landed — but the slot is a fixed 14px square, so
            nothing can shift whatever is in it, and the crossfade did not work:
            `.pulse-dot` animates its own opacity, a running animation beats an
            inline style, and so every finished row kept an orange dot beating
            over the top of its tick. */}
        {step.running ? (
          <span className="pulse-dot" />
        ) : (
          <DoneIcon
            size={13}
            weight="bold"
            style={{ color: step.ok ? undefined : "var(--warn)" }}
          />
        )}
      </span>
      {/* Shimmering only while it's the live one — a page of shimmering rows
          would be a page that never settles. */}
      <span className={step.running ? "shimmer" : undefined}>{step.label}</span>
      {/* Counting from a second in. It used to wait for three, on the grounds
          that a number is only interesting once you'd started to wonder — but a
          row that sits there with no number beside it is a row you can't tell
          from a stuck one, and by the time you are wondering you want to know
          it has been moving all along. Kept once the call has landed, because
          "that took nine seconds" is worth knowing. */}
      {step.running && elapsed >= 1 && <span className="tool-step-secs mono">{elapsed}s</span>}
      {!step.running && took !== null && took >= 1 && (
        <span className="tool-step-secs mono">{took}s</span>
      )}
    </div>
  );
}

/**
 * The turn is running and has nothing on screen to show for it.
 *
 * Two waits look like this, and until now both were blank. The seconds between
 * pressing send and the first tool call, where the model is deciding what to
 * read; and the longer one after the last call lands, while it writes the answer
 * but before the first word of it arrives. The turn's block was rendered for
 * both — an empty flex item under your question, a 34px gap, and no way to tell
 * a slow model from a dead one.
 *
 * Deliberately the same row as a tool call rather than a spinner of its own:
 * this is one more thing the turn is doing, in the same list, and it is replaced
 * in place by the call it turns into.
 */
export function Thinking() {
  // Mount time rather than the turn's start: this counts how long it has been
  // since there was anything to look at, which is the wait the number answers.
  // A tool call that took nine seconds keeps its own nine on its own row.
  const [since] = useState(() => Date.now());
  const elapsed = useElapsed(since, true);

  return (
    <div className="tool-step" data-running="true" role="status">
      <span className="tool-step-mark" aria-hidden>
        <span className="pulse-dot" />
      </span>
      <span className="shimmer">Thinking…</span>
      {/* Out of the accessible name: a number that changes every second is a
          screen reader interrupting itself once a second. */}
      {elapsed >= 1 && (
        <span className="tool-step-secs mono" aria-hidden>
          {elapsed}s
        </span>
      )}
    </div>
  );
}

/**
 * The same thing on a message that has already been answered: one line, and the
 * rows behind it if you want them.
 *
 * Built from the labels saved with the message rather than from live steps —
 * a conversation reopened tomorrow has no steps, and this is the part of the
 * turn worth keeping. Duplicates collapse: the model reading recent activities
 * twice in one turn is a detail about the model, not about your data.
 */
export function ToolSummary({ sources }: { sources: string[] }) {
  const [open, setOpen] = useState(false);
  const unique = [...new Set(sources)];
  if (unique.length === 0) return null;

  return (
    <>
      <button
        type="button"
        className="tool-summary"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
      >
        <Caret />
        {unique.length === 1 ? "Read one thing" : `Read ${unique.length} things`}
      </button>
      {/* Kept in the tree and collapsed to nothing rather than dropped. The
          caret beside the button already turns over `--dur-base`, and a hinge
          that opens smoothly onto content arriving between two frames is the
          interface promising a motion it doesn't perform — worse, the rows push
          the answer below them down the page as they land. See
          `.tool-summary-panel` for how the height is transitioned without
          anybody having to know what it is. */}
      <div className="tool-summary-panel" data-open={open || undefined} aria-hidden={!open}>
        <div className="tool-steps" style={{ marginTop: -6 }}>
          {unique.map((label) => (
            <div key={label} className="tool-step" data-running="false">
              <span className="tool-step-mark" aria-hidden>
                <DoneIcon size={13} weight="bold" />
              </span>
              <span>{label}</span>
            </div>
          ))}
        </div>
      </div>
    </>
  );
}

/** Rotated by CSS when the summary is open, so the two states share one glyph. */
function Caret() {
  return (
    <svg
      className="tool-summary-caret"
      width="9"
      height="9"
      viewBox="0 0 9 9"
      fill="none"
      aria-hidden
    >
      <path d="M2.5 1 L6.5 4.5 L2.5 8" stroke="currentColor" strokeWidth="1.4" />
    </svg>
  );
}
