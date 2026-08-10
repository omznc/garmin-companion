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
import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useRef,
  type ReactNode,
  type Ref,
} from "react";
import { createPortal } from "react-dom";
import { SendIcon } from "../../lib/icons";

/** What a screen holding a composer can do to it. */
export interface ComposerHandle {
  focus: () => void;
}

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
  /**
   * Whether the box takes the caret on its own.
   *
   * For the screen whose whole purpose is this box: arriving at Ask and having
   * to click into it before typing is a step that exists for no reason. Off by
   * default, because the strip under an activity is not that screen — you came
   * to read the session, and a composer that grabs the caret on the way in is
   * one that has decided for you what the page is for.
   *
   * Off on Android too, from the caller. Focus there raises the keyboard, which
   * would cover most of the screen you just navigated to before you had asked
   * for it.
   */
  autoFocus = false,
  /**
   * For the one thing the box can't see coming: starting a new conversation.
   *
   * Mounting and becoming usable again are both changes to the composer, so it
   * notices them itself. "New" is a change to the screen behind it — the same
   * empty page you arrive at, with the same next move — and only the screen
   * knows it happened.
   */
  ref,
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
  autoFocus?: boolean;
  ref?: Ref<ComposerHandle>;
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

  const focus = useCallback(() => {
    const el = area.current;
    if (!el || el.disabled) return;
    // `preventScroll` because the thread does its own scrolling — it sticks to
    // the bottom while an answer streams — and the browser's idea of bringing a
    // fixed element into view is a fight it would win at the wrong moment.
    el.focus({ preventScroll: true });
    // Caret after whatever is in the box rather than before it. A question
    // handed over from another screen arrives already written, and the useful
    // version of it usually has a clause of your own on the end.
    const end = el.value.length;
    el.setSelectionRange(end, end);
  }, []);

  useEffect(() => {
    if (autoFocus) focus();
  }, [autoFocus, focus]);

  useImperativeHandle(ref, () => ({ focus }), [focus]);

  /**
   * Take the caret back when the box stops being unusable — but only if it was
   * this box that lost it.
   *
   * Two things take focus away without you asking. `disabled` while a question
   * is on screen drops it to the body outright; and sending with the mouse
   * leaves it on the send button, so the next thing you type after an answer
   * goes nowhere. Both are the box's own doing and both are worth undoing.
   *
   * What is not worth undoing is focus that something else is *using*, which is
   * a narrower thing than focus something else merely has. A button you pressed
   * a moment ago holds the caret without needing it — the model's own question
   * card is exactly that, and refusing to move off it would mean the box never
   * came back after a turn that asked you something. Another text field, or
   * anything inside an open drawer, is different: there the caret is the point,
   * and taking it would interrupt you to offer you a second text box.
   */
  const was = useRef({ busy, blocked });
  useEffect(() => {
    const before = was.current;
    was.current = { busy, blocked };
    if (!autoFocus) return;
    if (!((before.blocked && !blocked) || (before.busy && !busy))) return;
    const active = document.activeElement;
    if (active instanceof HTMLElement && !box.current?.contains(active)) {
      const inUse =
        active.isContentEditable ||
        active instanceof HTMLInputElement ||
        active instanceof HTMLTextAreaElement ||
        active.closest('[role="dialog"]') !== null;
      if (inUse) return;
    }
    focus();
  }, [autoFocus, busy, blocked, focus]);

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
