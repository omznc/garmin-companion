/**
 * The phone's navigation: three tabs and a More button along the bottom, and a
 * sheet holding every other screen.
 *
 * A separate component from `Sidebar` rather than a responsive version of it,
 * because almost nothing survives the move. The sidebar is a reorderable list
 * with a drag gesture, a sync button, an update prompt and a palette swatch,
 * pinned full-height beside the content. A tab bar is four fixed destinations
 * whose positions are the entire benefit — muscle memory for "Today is
 * bottom-left" is worth more than any arrangement you could reach by dragging.
 *
 * So the two share `lib/nav`'s list and nothing else. What the sidebar has and
 * this doesn't lives in the sheet, which is where a phone expects to find it.
 *
 * # Which three
 *
 * `loadTabs()` — its own stored set with its own default, not the top of the
 * sidebar order. See the note there for why the phone disagrees with the
 * desktop about which three matter.
 *
 * They are changed by pressing and holding a screen in the sheet and dropping
 * it on the tab it should replace. Holding rather than tapping because every
 * row in that sheet is a link first, and dropping rather than "it goes in slot
 * three" because the finger saying which tab leaves is the only version of this
 * with no rule to remember.
 *
 * # Why this portals to `document.body`
 *
 * `#root` is what scrolls (see `lib/scroller.ts`), and on Android the overscroll
 * at the end of a scroll is a stretch applied to the whole scrolling layer —
 * including anything `position: fixed` inside it. Rendered in the tree, the tab
 * bar bounced along with the content it is supposed to stay still behind. Out
 * of the scroller it cannot be stretched, and the content keeps its bounce,
 * which is the half of that effect worth having.
 */
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { createPortal } from "react-dom";
import { Link, useRouterState } from "@tanstack/react-router";
import { loadNavOrder, loadTabs, saveTabs, type NavEntry } from "../lib/nav";
import { Spring, project, rubberband } from "../lib/spring";
import { MoreIcon } from "../lib/icons";

/** How long a press holds before it stops being a tap and becomes a promotion.
 *  Long enough not to fire on a slow tap, short enough that someone who was
 *  told to press and hold doesn't let go first. */
const HOLD_MS = 400;
/** How far the finger may drift inside that window and still count as held —
 *  the same slop a tap gets, because a still finger isn't actually still. */
const HOLD_SLOP = 9;

/** How far down the sheet has to be, once its release velocity is projected
 *  forward, before letting go means dismissing rather than settling back. */
const DISMISS_AT = 0.4;

/** Arriving under a tap. Critically damped: nothing preceded it that would
 *  justify overshoot, and a menu that bounces because it appeared reads as
 *  decoration. */
const ARRIVE = { damping: 1, response: 0.34 };
/** Leaving a finger that was moving. This one did carry momentum, so it is
 *  allowed the overshoot that makes it read as thrown rather than played. */
const THROWN = { damping: 0.82, response: 0.32 };

/** A short tick on the two moments that commit something. Meaningful only if
 *  it stays rare — a phone that buzzes on every tap trains you to stop
 *  noticing. Absent outside a WebView, and off entirely if the system says so. */
function tick(ms: number) {
  navigator.vibrate?.(ms);
}

const clamp = (v: number, lo: number, hi: number) => Math.min(Math.max(v, lo), hi);

/** A promotion in flight: which screen is in hand, and the geometry the drop
 *  test needs — measured once at lift, so no frame of the drag asks the browser
 *  for a rect it just invalidated. */
type Lift = {
  entry: NavEntry;
  /** The three destination tabs. More is not a slot; it has nowhere to go. */
  slots: DOMRect[];
  /** The band that counts as "over the bar", reaching above it so the drop
   *  doesn't demand precision from a thumb that is covering the target. */
  band: number;
  /** Where inside the row the finger grabbed. Snapping the chip to its own
   *  centre on lift is the single clearest way to break the illusion. */
  dx: number;
  dy: number;
  /** The row's own corner, so the chip's first frame is drawn over the thing it
   *  was lifted off rather than arriving from nowhere. */
  x: number;
  y: number;
};

export function TabBar() {
  const [tabs, setTabs] = useState<NavEntry[]>(loadTabs);
  const [sheet, setSheet] = useState(false);
  // Bumped to ask an open sheet to leave. A boolean would unmount it mid-air;
  // this lets it run its exit and report back when it has landed.
  const [bye, setBye] = useState(0);
  const [lift, setLift] = useState<Lift | null>(null);
  const [drop, setDrop] = useState<number | null>(null);
  const [announce, setAnnounce] = useState("");

  const bar = useRef<HTMLElement>(null);
  const chip = useRef<HTMLDivElement>(null);
  // The live drop slot, for the pointer handlers. `drop` is the same value on a
  // slower clock — it only exists to re-render the highlight.
  const over = useRef<number | null>(null);

  const pathname = useRouterState({ select: (s) => s.location.pathname });

  useEffect(() => saveTabs(tabs), [tabs]);

  const dismiss = useCallback(() => setBye((n) => n + 1), []);

  // Any navigation closes the sheet — including one to the screen already
  // showing, where nothing else would.
  useEffect(() => {
    dismiss();
  }, [pathname, dismiss]);

  // Nothing behind the sheet scrolls while it is open. Without this the page
  // slides around under a surface that is supposed to have taken over, and the
  // sheet's own drag competes with it for the same finger.
  useEffect(() => {
    if (!sheet) return;
    document.documentElement.dataset.sheet = "true";
    return () => {
      delete document.documentElement.dataset.sheet;
    };
  }, [sheet]);

  // The tab for the screen you're on. A path under a tab counts as that tab
  // (`/activities/123` lights Activities), so opening a detail screen doesn't
  // leave the whole bar looking unselected.
  const isActive = (to: string) => pathname === to || pathname.startsWith(`${to}/`);
  // The sheet is everything without a tab, in the order the nav is in. Read
  // once: there is no sidebar on a phone, so nothing can reorder it mid-session,
  // and a drag re-renders this component several times.
  const order = useMemo(loadNavOrder, []);
  const rest = order.filter((n) => !tabs.includes(n));
  const inSheet = rest.some((n) => isActive(n.to));

  /* ------------------------------------------------------------ promotion --- */

  /**
   * The hold fired. Measure the bar, put a chip under the finger, and take the
   * gesture over from the sheet — which retracts, because it is sitting on top
   * of the only thing this drag can be dropped on.
   */
  const startLift = (entry: NavEntry, e: PointerEvent, row: DOMRect) => {
    const nav = bar.current;
    if (!nav) return;
    const slots = Array.from(nav.children)
      .slice(0, tabs.length)
      .map((el) => el.getBoundingClientRect());

    tick(12);
    over.current = null;
    setDrop(null);
    setLift({
      entry,
      slots,
      band: nav.getBoundingClientRect().top - 28,
      dx: e.clientX - row.left,
      dy: e.clientY - row.top,
      x: row.left,
      y: row.top,
    });
    setAnnounce(`${entry.label} lifted. Drop it on a tab to replace it.`);
  };

  // The drag itself, on the window: the finger leaves the row it started on
  // immediately, and the chip has to keep following it when it does.
  useEffect(() => {
    if (!lift) return;

    const move = (e: PointerEvent) => {
      chip.current?.style.setProperty(
        "transform",
        `translate3d(${e.clientX - lift.dx}px,${e.clientY - lift.dy}px,0)`,
      );
      const slot =
        e.clientY >= lift.band
          ? lift.slots.findIndex((r) => e.clientX >= r.left && e.clientX <= r.right)
          : -1;
      const next = slot < 0 ? null : slot;
      if (next !== over.current) {
        over.current = next;
        setDrop(next);
        // A tick when it arms, not when it commits: this is the moment the
        // outcome becomes knowable, and the point of it is that you can still
        // change your mind.
        if (next !== null) tick(8);
      }
    };

    // `pointercancel` and `pointerup` can both arrive before the state change
    // below has torn these listeners down, and the second one would clear the
    // announcement the first one just made.
    let done = false;

    const up = () => {
      if (done) return;
      done = true;
      const slot = over.current;
      if (slot !== null) {
        tick(18);
        const gone = tabs[slot];
        setTabs((prev) => prev.map((t, i) => (i === slot ? lift.entry : t)));
        setAnnounce(`${lift.entry.label} is now tab ${slot + 1}. ${gone.label} moved to More.`);
        // The sheet has already retracted and the bar behind it has changed —
        // there is nothing left to animate away.
        setSheet(false);
      } else {
        setAnnounce("");
      }
      setLift(null);
      setDrop(null);
      over.current = null;
    };

    // Once the hold has fired, the finger belongs to this drag. Without this the
    // WebView reads the very same movement as a scroll of the list underneath,
    // takes the pointer, and fires `pointercancel` — which arrives as "let go"
    // and drops the chip on its first millimetre, a metre from any tab.
    //
    // It has to be a non-passive `touchmove` rather than `touch-action`, which
    // is consulted when the touch begins: that is 400ms before anyone knows
    // this one was a hold, and by then it is too late to change the answer.
    const claim = (e: TouchEvent) => e.preventDefault();

    window.addEventListener("touchmove", claim, { passive: false });
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("touchmove", claim);
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  }, [lift, tabs]);

  // Seeded before the first paint, or the chip appears at the origin for a
  // frame and flies in from the corner.
  useLayoutEffect(() => {
    const el = chip.current;
    if (!lift || !el) return;
    el.style.transform = `translate3d(${lift.x}px,${lift.y}px,0)`;
  }, [lift]);

  /* ----------------------------------------------------------------- bar --- */

  const LiftIcon = lift?.entry.icon;

  return createPortal(
    <>
      {sheet && (
        <Sheet
          entries={rest}
          bye={bye}
          lifting={lift !== null}
          onLift={startLift}
          onClosed={() => setSheet(false)}
        />
      )}

      <nav
        ref={bar}
        className="tabbar"
        aria-label="Main"
        // Over the scrim for the length of a drop, and only then: the sheet has
        // to be able to cover it the rest of the time.
        data-lifting={lift ? "true" : undefined}
      >
        {tabs.map((n, i) => (
          <Tab
            key={n.to}
            to={n.to}
            label={n.label}
            icon={n.icon}
            active={isActive(n.to)}
            drop={drop === i}
          />
        ))}
        <button
          type="button"
          className="tab"
          data-active={sheet || inSheet || undefined}
          aria-expanded={sheet}
          aria-haspopup="menu"
          onClick={() => (sheet ? dismiss() : setSheet(true))}
        >
          <MoreIcon size={21} aria-hidden />
          <span>More</span>
        </button>
      </nav>

      {lift && LiftIcon && (
        <div ref={chip} className="lift-chip" aria-hidden>
          <LiftIcon size={18} />
          {lift.entry.label}
        </div>
      )}

      <span className="sr-only" aria-live="polite">
        {announce}
      </span>
    </>,
    document.body,
  );
}

function Tab({
  to,
  label,
  icon: Icon,
  active,
  drop,
}: {
  to: NavEntry["to"];
  label: string;
  icon: NavEntry["icon"];
  active: boolean;
  drop: boolean;
}) {
  return (
    <Link to={to} className="tab" data-active={active || undefined} data-drop={drop || undefined}>
      <Icon size={21} aria-hidden />
      <span>{label}</span>
    </Link>
  );
}

/**
 * The rest of the screens, as a sheet from the bottom.
 *
 * From the bottom because that is the half of a phone a thumb reaches. A
 * centred dialog would put the last item — Settings — furthest from the hand
 * that opened it.
 *
 * # Why the motion is a spring and not a keyframe
 *
 * The first version was `@keyframes` in, and nothing at all out: it appeared
 * over a quarter-second and then vanished between two frames, which reads as a
 * crash rather than a dismissal. Both halves are one spring now, and the reason
 * it is a spring rather than a pair of transitions is that this surface can be
 * grabbed. A sheet already on its way out that you catch has to follow the
 * finger from wherever it currently is — a transition would either finish first
 * or jump. The spring's position *is* the drag's position, so there is no seam
 * between them and no state where input is locked out.
 *
 * One spring drives both surfaces: the panel's offset below rest, and the
 * scrim's opacity as one minus its fraction of the way down. Two would let the
 * dimming disagree with the drag it is supposed to be reporting.
 */
function Sheet({
  entries,
  bye,
  lifting,
  onLift,
  onClosed,
}: {
  entries: NavEntry[];
  bye: number;
  lifting: boolean;
  onLift: (entry: NavEntry, e: PointerEvent, row: DOMRect) => void;
  onClosed: () => void;
}) {
  const panel = useRef<HTMLDivElement>(null);
  const scrim = useRef<HTMLDivElement>(null);
  const height = useRef(1);
  const closing = useRef(false);
  const closed = useRef(onClosed);
  closed.current = onClosed;

  // Built once and kept: it holds the live position, which is the thing every
  // interruption has to resume from.
  const [spring] = useState(
    () =>
      new Spring((y) => {
        const p = panel.current;
        const s = scrim.current;
        if (p) p.style.transform = `translate3d(0,${y}px,0)`;
        if (s) s.style.opacity = String(clamp(1 - y / height.current, 0, 1));
        // Reported from inside the motion rather than on a timer, so a
        // dismissal that was flicked hard unmounts when it actually lands and
        // not a fixed duration later.
        if (closing.current && y >= height.current - 0.5) closed.current();
      }),
  );

  const exit = useCallback(
    (velocity = 0) => {
      closing.current = true;
      spring.to(height.current, velocity, velocity ? THROWN : ARRIVE);
      // A spring already at its target never ticks, so it would never report.
      if (spring.x >= height.current - 0.5) closed.current();
    },
    [spring],
  );

  // Measured and placed off-screen before the first paint, then released — the
  // sheet must never be visible at rest for a frame on its way in.
  useLayoutEffect(() => {
    height.current = Math.max(panel.current?.offsetHeight ?? 0, 1);
    spring.set(height.current);
    spring.to(0, 0, ARRIVE);
  }, [spring]);

  // A lift is a drag onto the tab bar, which is underneath this. Out of the way
  // rather than dismissed: if the drop is abandoned the sheet comes back, and
  // it should come back to the list you were reading.
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    if (closing.current) return;
    spring.to(lifting ? height.current : 0, 0, ARRIVE);
  }, [lifting, spring]);

  // The parent asking it to leave — a tap on a sheet item, a tap on More, a
  // navigation from anywhere else.
  const askedAt = useRef(bye);
  useEffect(() => {
    if (bye !== askedAt.current) exit();
  }, [bye, exit]);

  // Escape closes it. Rare on a phone, but this build also runs under
  // `tauri android dev` on a desktop browser, where it is the reflex.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") exit();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [exit]);

  // Focus moves into the sheet so the back gesture and a screen reader both
  // treat it as the current surface rather than an overlay nobody is in.
  useEffect(() => {
    panel.current?.focus();
  }, []);

  /* -------------------------------------------------------- drag to close --- */

  // Only from the handle strip. The list below it scrolls, and a sheet that
  // reads every downward drag as a dismissal makes a scrollable list unusable;
  // deciding between the two per-gesture is a guess, and a wrong guess either
  // drops the sheet or refuses to move it.
  const drag = useRef<{ y: number; t: number; v: number } | null>(null);

  const grab = (e: ReactPointerEvent) => {
    // Caught mid-flight, from wherever it currently is — a sheet still on its
    // way in that you grab must follow the finger, not finish arriving first.
    spring.stop();
    drag.current = { y: e.clientY - spring.x, t: e.timeStamp, v: 0 };
    e.currentTarget.setPointerCapture(e.pointerId);
  };

  const pull = (e: ReactPointerEvent) => {
    const d = drag.current;
    if (!d) return;
    const raw = e.clientY - d.y;
    // Up is a boundary, not a direction: there is no more sheet above the top
    // of it. Resistance rather than a wall, so it still answers the finger.
    const y = raw >= 0 ? raw : -rubberband(-raw, height.current);
    const dt = Math.max(e.timeStamp - d.t, 1);
    d.v = ((y - spring.x) / dt) * 1000;
    d.t = e.timeStamp;
    spring.set(y);
  };

  const release = () => {
    const d = drag.current;
    if (!d) return;
    drag.current = null;
    // Where the flick was going, not where the finger stopped. A short fast
    // swipe and a long slow drag can end on the same pixel and mean opposite
    // things; only the velocity tells them apart.
    const landing = spring.x + project(d.v, 0.99);
    // The release velocity is handed to whichever spring takes over, so there
    // is no seam between the drag and the motion that finishes it.
    if (landing > height.current * DISMISS_AT) exit(d.v);
    else spring.to(0, d.v, THROWN);
  };

  /* ------------------------------------------------------------ the hold --- */

  const hold = useRef<{ timer: number; x: number; y: number } | null>(null);
  // A hold that fired ends on a row that is also a link. Without this, letting
  // go navigates to the screen you just finished rearranging.
  const swallow = useRef(false);

  const press = (e: ReactPointerEvent, entry: NavEntry) => {
    const row = e.currentTarget.getBoundingClientRect();
    const native = e.nativeEvent;
    swallow.current = false;
    hold.current = {
      x: e.clientX,
      y: e.clientY,
      timer: window.setTimeout(() => {
        hold.current = null;
        swallow.current = true;
        onLift(entry, native, row);
      }, HOLD_MS),
    };
  };

  const drift = (e: ReactPointerEvent) => {
    const h = hold.current;
    if (!h) return;
    // Moving means this was a scroll or a slip, not a hold. Cancelled rather
    // than tracked, so the list still scrolls the way a list should.
    if (Math.hypot(e.clientX - h.x, e.clientY - h.y) > HOLD_SLOP) letGo();
  };

  const letGo = () => {
    if (hold.current) clearTimeout(hold.current.timer);
    hold.current = null;
  };

  return (
    <div
      ref={scrim}
      className="sheet-scrim"
      // Opacity is written by the spring from the first frame; this is only the
      // value it starts from, so the scrim is never briefly opaque.
      style={{ opacity: 0 }}
      onClick={() => exit()}
    >
      {/* Stops a tap inside the panel closing it on the way back up. */}
      <div
        ref={panel}
        className="sheet"
        role="menu"
        tabIndex={-1}
        onClick={(e) => e.stopPropagation()}
      >
        <div
          className="sheet-handle"
          onPointerDown={grab}
          onPointerMove={pull}
          onPointerUp={release}
          onPointerCancel={release}
        >
          {/* The handle every bottom sheet on the platform has, and here it is
              load-bearing: this strip is the part you can drag. */}
          <div className="sheet-grip" aria-hidden />
        </div>

        <div className="sheet-list">
          {entries.map((n) => {
            const Icon = n.icon;
            return (
              <Link
                key={n.to}
                to={n.to}
                role="menuitem"
                className="sheet-item"
                onPointerDown={(e) => press(e, n)}
                onPointerMove={drift}
                onPointerUp={letGo}
                onPointerCancel={letGo}
                onClick={(e) => {
                  if (swallow.current) e.preventDefault();
                }}
              >
                <Icon size={18} aria-hidden />
                {n.label}
              </Link>
            );
          })}
        </div>

        {/* Press-and-hold is invisible until someone is told about it, and a
            feature nobody finds may as well not have shipped. */}
        <p className="sheet-hint">Press and hold a screen to give it a tab.</p>
      </div>
    </div>
  );
}
