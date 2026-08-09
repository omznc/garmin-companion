/**
 * A surface that slides in over the page, from whichever edge a hand is near.
 *
 * On a phone that is the bottom, where a thumb is. On a desktop it is the
 * right: the left is where the nav already lives, and a second column of links
 * on the other side of the reading measure turns the window into a maze.
 *
 * # Why the motion is a spring and not a keyframe
 *
 * The same reason the phone's nav sheet is one (`TabBar`): this can be grabbed.
 * A panel already on its way out that you catch has to follow the finger from
 * wherever it currently is, and a transition would either finish first or jump.
 * The spring's position *is* the drag's position, so there is no seam between
 * them and no state where input is locked out.
 *
 * One spring drives both surfaces — the panel's offset past its rest position,
 * and the scrim's opacity as one minus its fraction of the way out. Two would
 * let the dimming disagree with the drag it is reporting.
 *
 * This is a second sheet in the codebase and not a generalisation of the first.
 * `TabBar`'s also hosts the press-and-hold gesture that promotes a screen to a
 * tab, which is most of its length and none of its job as a surface; pulling
 * the two apart is worth doing on the day something needs a third.
 */
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";
import { Spring, project, rubberband } from "../lib/spring";
import { onBack } from "../lib/back";
import { IS_MOBILE } from "../lib/platform";

/** How far out it has to be, once release velocity is projected forward,
 *  before letting go means dismissing rather than settling back. */
const DISMISS_AT = 0.4;

/** Arriving under a tap: critically damped, because nothing threw it. */
const ARRIVE = { damping: 1, response: 0.34 };
/** Leaving a finger that was moving, which is allowed its overshoot. */
const THROWN = { damping: 0.82, response: 0.32 };

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

/** What Tab is allowed to land on. Queried per keypress rather than cached: the
 *  list inside this can be loading, empty, or a dozen rows long by the time
 *  anybody reaches for the keyboard. */
const FOCUSABLE =
  'a[href],button:not(:disabled),input:not(:disabled),textarea:not(:disabled),select:not(:disabled),[tabindex]:not([tabindex="-1"])';

export function Drawer({
  title,
  onClose,
  children,
}: {
  title: string;
  /** Called once it has finished leaving, not when the dismissal starts. */
  onClose: () => void;
  children: ReactNode;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const scrim = useRef<HTMLDivElement>(null);
  /** The travel: the panel's height on a phone, its width on a desktop. */
  const span = useRef(1);
  const closing = useRef(false);
  const closed = useRef(onClose);
  closed.current = onClose;

  // Built once and kept: it holds the live position, which is what every
  // interruption has to resume from.
  const [spring] = useState(
    () =>
      new Spring((x) => {
        const p = panel.current;
        const s = scrim.current;
        if (p)
          p.style.transform = IS_MOBILE ? `translate3d(0,${x}px,0)` : `translate3d(${x}px,0,0)`;
        if (s) s.style.opacity = String(clamp(1 - x / span.current, 0, 1));
        // Reported from inside the motion rather than on a timer, so one that
        // was flicked hard unmounts when it lands and not a fixed duration later.
        if (closing.current && x >= span.current - 0.5) closed.current();
      }),
  );

  const exit = useCallback(
    (velocity = 0) => {
      closing.current = true;
      spring.to(span.current, velocity, velocity ? THROWN : ARRIVE);
      // A spring already at its target never ticks, so it would never report.
      if (spring.x >= span.current - 0.5) closed.current();
    },
    [spring],
  );

  // Measured and placed off-screen before the first paint, then released — it
  // must never be visible at rest for a frame on its way in.
  useLayoutEffect(() => {
    const el = panel.current;
    span.current = Math.max((IS_MOBILE ? el?.offsetHeight : el?.offsetWidth) ?? 0, 1);
    spring.set(span.current);
    spring.to(0, 0, ARRIVE);
  }, [spring]);

  // Escape leaves, and Tab stays inside.
  //
  // `aria-modal` below tells a screen reader that the rest of the page is not
  // there. Without the second half of this that was a claim and not a fact: Tab
  // walked straight out of the panel and into the conversation behind it, and
  // the only way back was to keep tabbing until it came round again. Wrapping at
  // both ends is what makes the scrim mean something to a keyboard.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") return exit();
      if (e.key !== "Tab") return;
      const el = panel.current;
      if (!el) return;
      const stops = Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE));
      // The panel itself is the fallback stop — an empty list has nothing to
      // hold focus, and letting it escape then would be the bug all over again.
      const first = stops[0] ?? el;
      const last = stops[stops.length - 1] ?? el;
      const at = document.activeElement;
      if (!el.contains(at)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      } else if (e.shiftKey && (at === first || at === el)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && at === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [exit]);

  // Back closes this rather than leaving the screen under it.
  useEffect(() => onBack(() => (exit(), true)), [exit]);

  // Nothing behind it scrolls while it is open, or the page slides around under
  // a surface that has supposedly taken over.
  useEffect(() => {
    document.documentElement.dataset.sheet = "true";
    return () => {
      delete document.documentElement.dataset.sheet;
    };
  }, []);

  // Focus moves in, so the back gesture and a screen reader both treat this as
  // the current surface rather than as an overlay nobody is in — and goes back
  // where it came from on the way out. Something opened this; dismissing it
  // should leave you at that button and not at the top of the document, which
  // is where focus falls when the element holding it is removed.
  useEffect(() => {
    const from = document.activeElement;
    panel.current?.focus();
    return () => {
      if (from instanceof HTMLElement && from.isConnected) from.focus();
    };
  }, []);

  /* -------------------------------------------------------- drag to close --- */

  // From the handle only, on the phone. The list below it scrolls, and a sheet
  // that reads every drag as a dismissal makes a scrollable list unusable.
  const drag = useRef<{ from: number; t: number; v: number } | null>(null);

  const axis = (e: { clientX: number; clientY: number }) => (IS_MOBILE ? e.clientY : e.clientX);

  const grab = (e: React.PointerEvent) => {
    // Caught mid-flight, from wherever it is — one still arriving that you grab
    // must follow the finger rather than finish arriving first.
    spring.stop();
    drag.current = { from: axis(e) - spring.x, t: e.timeStamp, v: 0 };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const pull = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const raw = axis(e) - d.from;
    // Back past rest is a boundary, not a direction: there is no more panel
    // there. Resistance rather than a wall, so it still answers the finger.
    const x = raw >= 0 ? raw : -rubberband(-raw, span.current);
    const dt = Math.max(e.timeStamp - d.t, 1);
    d.v = ((x - spring.x) / dt) * 1000;
    d.t = e.timeStamp;
    spring.set(x);
  };

  const release = () => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    // Where the flick was going, not where the finger stopped: a short fast
    // swipe and a long slow drag can end on the same pixel and mean opposite
    // things, and only the velocity tells them apart.
    if (spring.x + project(d.v, 0.99) > span.current * DISMISS_AT) exit(d.v);
    else spring.to(0, d.v, THROWN);
  };

  return createPortal(
    <div
      ref={scrim}
      className="drawer-scrim"
      // Written by the spring from the first frame; this is only the value it
      // starts from, so the scrim is never briefly opaque.
      style={{ opacity: 0 }}
      onClick={() => exit()}
    >
      <div
        ref={panel}
        className={`drawer ${IS_MOBILE ? "drawer-sheet" : "drawer-side"}`}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
      >
        {IS_MOBILE && (
          <div
            className="sheet-handle"
            onPointerDown={grab}
            onPointerMove={pull}
            onPointerUp={release}
            onPointerCancel={release}
          >
            <div className="sheet-grip" aria-hidden />
          </div>
        )}
        {children}
      </div>
    </div>,
    document.body,
  );
}
