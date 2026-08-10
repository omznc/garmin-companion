/**
 * Pull the top of a phone screen down to sync it.
 *
 * # Why this exists twice over
 *
 * The obvious half is that it's the gesture a phone has for "get me this
 * again", and the app's refresh was a caption-sized button up in the corner of
 * the header — reachable, but nobody's first instinct and nowhere near a thumb.
 *
 * The less obvious half is what it *replaces*. On a phone that stretches at the
 * end of a scroll, dragging down from the top of the page overscrolled the
 * document, and Android's stretch is applied to the whole root layer —
 * `position: fixed` children included. The tab bar lives in `document.body` to
 * stay out of `#root`'s stretch (see the note in `TabBar`), but the document's
 * own stretch caught it anyway, and the nav bounced every time you flicked back
 * to the top. Here that drag is claimed before the WebView sees it — the
 * `touchmove` handler is non-passive and calls `preventDefault`, so there is no
 * overscroll to stretch anything. The stylesheet turns the document's effect off
 * as well, for the screens with nothing to refresh, where there is no gesture to
 * claim it instead.
 *
 * # The shape of it
 *
 * Material's, not iOS's: the content stays where it is and a circular spinner
 * descends from behind the top edge. That is the convention on this platform,
 * and it avoids transforming the shell — which would make it the containing
 * block for every `position: fixed` thing inside it, composer included.
 *
 * The spinner is the whole of the feedback while you drag. Once it commits, the
 * sync narrates itself in the card above the tab bar like any other, so there is
 * nothing more for this to say.
 *
 * Position is written straight to the node from a spring rather than held in
 * state: it moves every frame of a drag, and re-rendering the tree to move one
 * translate would be the expensive part of the gesture.
 */
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { scroller } from "../lib/scroller";
import { Spring, rubberband } from "../lib/spring";
import { canRefresh, refreshNow } from "../lib/refreshable";
import { SyncIcon } from "../lib/icons";

/** How far the finger travels before this is a pull rather than a still hand.
 *  Subtracted from the distance too, so the spinner starts from its hiding
 *  place rather than jumping out by the slop the moment it's exceeded. */
const SLOP = 12;
/** How far it has to come to commit. Far enough that a flick past the top of a
 *  list doesn't sync, close enough that a deliberate pull doesn't feel long. */
const TRIGGER = 76;
/** Where the spinner waits while the sync runs — just above the trigger point,
 *  so releasing settles it upwards a little instead of leaving it mid-drag. */
const REST = 60;
/** The ceiling, past which pulling harder does nothing but resist. */
const MAX = 132;

/** Coming back, or settling to the waiting position. Critically damped: this is
 *  a control returning to rest, not something thrown. */
const SETTLE = { damping: 1, response: 0.34 };

/**
 * Where the spinner is, for a finger that has travelled `over` past the slop.
 *
 * One-to-one up to the trigger and resistant after it. Rubberbanding the whole
 * distance was the first version and it was wrong here: resistance from the
 * first pixel means the spinner lags the finger the entire way, and reaching the
 * commit point took most of the screen. The finger should arrive where it is
 * aiming; what should push back is asking for more than there is, which past the
 * trigger is exactly what it is doing.
 *
 * The excess is banded against the room that's left, so the whole thing
 * approaches `MAX` and never passes it — no clamp, and therefore no point where
 * it stops answering the hand.
 */
function travel(over: number): number {
  // Dragging back up past where it started is a change of mind, not a push
  // upwards — the gesture is still ours, the spinner has simply gone home.
  if (over <= 0) return 0;
  if (over <= TRIGGER) return over;
  return TRIGGER + rubberband(over - TRIGGER, MAX - TRIGGER, 1);
}

/**
 * Whether the touch landed in something with its own scrollbar — a code block,
 * a drawer's list — which is entitled to its gesture. Walks up to the scroller
 * and no further; `#root` is the page and is exactly what this is for.
 */
function insideScroller(target: EventTarget | null): boolean {
  const box = scroller();
  let el = target instanceof Element ? target : null;
  while (el && el !== box) {
    if (el.scrollHeight > el.clientHeight + 1) {
      const overflow = getComputedStyle(el).overflowY;
      if (overflow === "auto" || overflow === "scroll") return true;
    }
    el = el.parentElement;
  }
  return false;
}

export function PullRefresh() {
  const dial = useRef<HTMLDivElement>(null);
  const [busy, setBusy] = useState(false);
  // The same fact the gesture needs from inside an event handler, where the
  // render's copy of it is a closure ago.
  const running = useRef(false);

  const [spring] = useState(
    () =>
      new Spring((y) => {
        const el = dial.current;
        if (!el) return;
        el.style.transform = `translate3d(-50%, ${y}px, 0)`;
        // Fully there well before the trigger, so the thing you are aiming at
        // is solid by the time it matters.
        el.style.opacity = String(Math.min(y / (TRIGGER * 0.55), 1));
        // The arrow turns with the pull, which is the only part of this that
        // says how far is far enough — a full turn lands on the trigger.
        el.style.setProperty("--turn", `${(y / TRIGGER) * 360}deg`);
      }, SETTLE),
  );

  useEffect(() => {
    const box = scroller();

    /** The touch began somewhere a pull is possible. */
    let armed = false;
    /** It has become a pull, and the WebView is no longer getting these. */
    let active = false;
    let startY = 0;
    let startX = 0;

    const begin = (e: TouchEvent) => {
      armed = false;
      active = false;
      if (running.current) return;
      // Two fingers is a pinch or a stray thumb, not a pull.
      if (e.touches.length !== 1) return;
      // The sheet is the surface while it's open; nothing behind it moves.
      if (document.documentElement.dataset.sheet) return;
      if (box.scrollTop > 0) return;
      if (!canRefresh()) return;
      if (insideScroller(e.target)) return;
      armed = true;
      startY = e.touches[0].clientY;
      startX = e.touches[0].clientX;
    };

    const move = (e: TouchEvent) => {
      if (!armed) return;
      const touch = e.touches[0];
      const dy = touch.clientY - startY;
      const dx = touch.clientX - startX;

      if (!active) {
        // Still deciding. Anything that isn't a downward drag hands the gesture
        // back for good rather than each frame — a scroll that passes back
        // through the origin must not turn into a pull halfway down the page.
        if (dy < 0 || Math.abs(dx) > Math.abs(dy)) {
          armed = false;
          return;
        }
        if (dy < SLOP) return;
        active = true;
      }

      // Ours from here: no native overscroll, so nothing stretches.
      e.preventDefault();
      spring.set(travel(dy - SLOP));
    };

    const end = () => {
      if (!active) {
        armed = false;
        return;
      }
      armed = false;
      active = false;

      if (spring.x < TRIGGER) {
        spring.to(0, 0, SETTLE);
        return;
      }

      // A tick on the one moment that commits, the same length `TabBar` uses
      // for a drop. Absent outside a WebView, and off entirely if the system
      // says so.
      navigator.vibrate?.(12);
      running.current = true;
      setBusy(true);
      spring.to(REST, 0, SETTLE);
      void refreshNow().finally(() => {
        running.current = false;
        setBusy(false);
        spring.to(0, 0, SETTLE);
      });
    };

    // `touchmove` alone is non-passive — it is the one that has to be able to
    // say no to the WebView. Declaring the others passive keeps them off the
    // scroll's critical path.
    box.addEventListener("touchstart", begin, { passive: true });
    box.addEventListener("touchmove", move, { passive: false });
    box.addEventListener("touchend", end, { passive: true });
    box.addEventListener("touchcancel", end, { passive: true });
    return () => {
      box.removeEventListener("touchstart", begin);
      box.removeEventListener("touchmove", move);
      box.removeEventListener("touchend", end);
      box.removeEventListener("touchcancel", end);
      spring.stop();
    };
  }, [spring]);

  // Out to the body, for the reason `TabBar` and `SyncBar` do it: this is the
  // one thing on screen that must not move with the page it is reporting on.
  return createPortal(
    <div
      ref={dial}
      className="pull-dial"
      role="status"
      // Only worth announcing once it is actually doing something — a spinner
      // following a finger has nothing to tell a screen reader.
      aria-label={busy ? "Syncing" : undefined}
      aria-hidden={!busy}
      data-busy={busy || undefined}
    >
      <SyncIcon size={15} className={busy ? "spin" : undefined} aria-hidden />
    </div>,
    document.body,
  );
}
