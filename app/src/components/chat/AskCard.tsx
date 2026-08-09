/**
 * The model's own question, put to you mid-answer.
 *
 * The turn is genuinely parked while this is on screen — the Rust side is
 * awaiting a channel inside the tool call that drew it (see `chat::ask_athlete`)
 * — so this is the one thing in the transcript that isn't there to be read. That
 * is why it gets a border and a ground when nothing else here does: a question
 * that looks like prose is a question you scroll past while the coach waits.
 *
 * Two states, and the same component draws both. Live: buttons, a text field for
 * an answer that isn't on the list, keys 1–4. Settled: the same card with what
 * was picked marked, which is what a reopened conversation shows. An unanswered
 * one says so rather than disappearing — the answer above it was written without
 * it, and that is worth being able to see.
 */
import { useEffect, useRef, useState } from "react";
import type { AskOption } from "../../lib/api";
import { SendIcon } from "../../lib/icons";

export function AskCard({
  header,
  question,
  options,
  multi,
  /** Null while it is still being asked; the chosen answers once it isn't. */
  answers,
  onAnswer,
}: {
  header?: string;
  question: string;
  options: AskOption[];
  multi: boolean;
  answers: string[] | null;
  onAnswer?: (answers: string[]) => void;
}) {
  const live = answers === null && onAnswer !== undefined;
  const [picked, setPicked] = useState<string[]>([]);
  const [own, setOwn] = useState("");
  const first = useRef<HTMLButtonElement>(null);

  // Focus lands on the first option as the card appears. The turn is waiting on
  // this and nothing else on the screen can move it forward, so taking the
  // caret out of the composer is the honest thing to do — and it makes the
  // number keys below reachable without a click first.
  useEffect(() => {
    if (live) first.current?.focus();
  }, [live]);

  // 1–4 pick an option. A keyboard shortcut for something that is only on
  // screen while it is being asked, which is when a shortcut can't collide with
  // anything else.
  useEffect(() => {
    if (!live) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      const target = e.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA") return;
      const n = Number(e.key);
      if (!Number.isInteger(n) || n < 1 || n > options.length) return;
      e.preventDefault();
      choose(options[n - 1].label);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  function choose(label: string) {
    if (!onAnswer) return;
    // One pick is the answer. Several needs a Send, because there is no way to
    // know from the click whether you have finished choosing.
    if (!multi) return void onAnswer([label]);
    setPicked((prev) =>
      prev.includes(label) ? prev.filter((p) => p !== label) : [...prev, label],
    );
  }

  function submit() {
    if (!onAnswer) return;
    const typed = own.trim();
    const out = typed ? [...picked, typed] : picked;
    if (out.length > 0) onAnswer(out);
  }

  const chosen = answers ?? [];
  const listed = new Set(options.map((o) => o.label));
  // Something typed rather than picked, on a settled card. Shown as its own row
  // so an answer the model didn't offer isn't quietly lost from the record.
  const typedAnswers = chosen.filter((a) => !listed.has(a));

  return (
    <div className="ask-card" data-live={live} role="group" aria-label={question}>
      {header && (
        <div className="eyebrow" style={{ marginBottom: 8 }}>
          {header}
        </div>
      )}
      <p className="ask-question">{question}</p>

      <div className="ask-options">
        {options.map((o, i) => (
          <button
            key={o.label}
            ref={i === 0 ? first : undefined}
            type="button"
            className="ask-option"
            disabled={!live}
            data-picked={live ? picked.includes(o.label) : chosen.includes(o.label)}
            aria-pressed={multi ? picked.includes(o.label) : undefined}
            onClick={() => choose(o.label)}
          >
            <div className="ask-option-label">{o.label}</div>
            {o.description && <div className="ask-option-desc">{o.description}</div>}
          </button>
        ))}
      </div>

      {live ? (
        <>
          {/* Always offered, so the model never has to spend an option on
              "something else" — and so a question with four wrong answers is
              still answerable. */}
          <div className="ask-own">
            <input
              className="input"
              value={own}
              placeholder="Or answer in your own words…"
              onChange={(e) => setOwn(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter") return;
                e.preventDefault();
                submit();
              }}
            />
            <button
              type="button"
              className="composer-send"
              disabled={picked.length === 0 && !own.trim()}
              aria-label="Send this answer"
              onClick={submit}
            >
              <SendIcon size={15} aria-hidden />
            </button>
          </div>
          {multi && <div className="ask-foot">Pick as many as apply, then send.</div>}
        </>
      ) : (
        <>
          {typedAnswers.length > 0 && (
            <div className="ask-foot">You said: {typedAnswers.join(", ")}</div>
          )}
          {chosen.length === 0 && (
            <div className="ask-foot">You didn't answer this one — it carried on without it.</div>
          )}
        </>
      )}
    </div>
  );
}
