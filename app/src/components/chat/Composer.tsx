/**
 * The box you type in, docked to the bottom of the reading column.
 *
 * Everything about a chat screen follows from where this sits. At the top —
 * where this app had it — the transcript has to run backwards to stay next to
 * it, and every answer pushes the one before it down the page. At the bottom,
 * the conversation reads in the order it happened and the newest thing is
 * always the thing nearest your hands. That is the whole change; the rest of
 * this file is the consequences.
 *
 * Portalled to `document.body` for the reason the tab bar is: `#root` is what
 * scrolls, and Android stretches the entire scrolling layer on overscroll —
 * `position: fixed` children included. Left in the tree, this bounced along
 * with the conversation it is supposed to sit still in front of.
 */
import { useEffect, useLayoutEffect, useRef, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { SendIcon } from "../../lib/icons";

/** How tall the textarea may grow before it starts scrolling instead. Matches
 *  the `max-height` in the stylesheet; both are here so neither is a surprise. */
const MAX_ROWS_PX = 190;

export function Composer({
  value,
  onChange,
  onSend,
  onStop,
  busy,
  /** Set while the model is waiting on an answer to a question it asked. */
  blocked = false,
  placeholder = "Ask about your training…",
  /** The suggestion row, drawn above the box. */
  above,
  /** The provider note, drawn under it. */
  note,
}: {
  value: string;
  onChange: (v: string) => void;
  onSend: () => void;
  onStop: () => void;
  busy: boolean;
  blocked?: boolean;
  placeholder?: string;
  above?: ReactNode;
  note?: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  // Grown to fit what's in it. Reset to `auto` first, or the height only ever
  // ratchets upwards — `scrollHeight` of an element already tall enough to hold
  // its content is that height, not the height it would need.
  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_ROWS_PX)}px`;
  }, [value]);

  // How much room to leave at the bottom of the thread. Published as a variable
  // rather than passed down, because the two things that need it — the thread's
  // padding and the jump pill's offset — are nowhere near this in the tree.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const write = () =>
      document.documentElement.style.setProperty("--composer-h", `${el.offsetHeight}px`);
    write();
    const ro = new ResizeObserver(write);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // The on-screen keyboard, on the platforms that have one.
  //
  // `interactive-widget=resizes-content` in the viewport meta covers the common
  // case by shrinking the layout viewport, but Android WebViews disagree about
  // it by version, and the failure mode — typing into a box under the keyboard —
  // is bad enough to be worth a belt as well as braces. The visual viewport
  // always knows, so the difference between the two viewports is written out as
  // a variable and the dock lifts by it.
  useEffect(() => {
    const vv = window.visualViewport;
    if (!vv) return;
    const write = () => {
      const covered = Math.max(0, window.innerHeight - vv.height - vv.offsetTop);
      document.documentElement.style.setProperty("--kb", `${Math.round(covered)}px`);
    };
    write();
    vv.addEventListener("resize", write);
    vv.addEventListener("scroll", write);
    return () => {
      vv.removeEventListener("resize", write);
      vv.removeEventListener("scroll", write);
      document.documentElement.style.setProperty("--kb", "0px");
    };
  }, []);

  const canSend = value.trim().length > 0 && !busy && !blocked;

  return createPortal(
    <div className="composer">
      {above}
      <div className="composer-box" ref={box}>
        <textarea
          ref={area}
          rows={1}
          value={value}
          placeholder={blocked ? "Answer the question above to carry on…" : placeholder}
          disabled={blocked}
          aria-label="Your question"
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.shiftKey) return;
            // Mid-composition Enter is the IME committing a character, not a
            // send. Without this, typing anything through an input method sends
            // half a word.
            if (e.nativeEvent.isComposing) return;
            e.preventDefault();
            if (canSend) onSend();
          }}
        />
        {/* One button, two jobs. While a turn is running it stops it — a
            question you regret asking costs real money on a hosted provider,
            and watching it arrive with no way to interrupt is the thing every
            other chat app fixed years ago. */}
        <button
          type="button"
          className="composer-send"
          data-stop={busy || undefined}
          disabled={!busy && !canSend}
          aria-label={busy ? "Stop" : "Send"}
          onClick={() => (busy ? onStop() : onSend())}
        >
          {/* Keyed, so React replaces the span rather than swapping the child
              inside it and the mark plays its way in. The background under it
              already eases between the two states; a glyph that cuts while the
              surface it sits on fades reads as two buttons, one of which
              blinked, instead of one changing its mind. */}
          <span key={busy ? "stop" : "send"} className="composer-mark">
            {busy ? <StopMark /> : <SendIcon size={15} aria-hidden />}
          </span>
        </button>
      </div>
      {note && <div className="composer-note">{note}</div>}
    </div>,
    document.body,
  );
}

/** A square. Nothing in the icon set is this, and nothing else should be. */
function StopMark() {
  return (
    <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden>
      <rect width="11" height="11" rx="2" fill="currentColor" />
    </svg>
  );
}
