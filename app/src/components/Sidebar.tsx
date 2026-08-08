import { Fragment, useEffect, useLayoutEffect, useRef, useState } from "react";
import { flushSync } from "react-dom";
import { Link, useRouterState } from "@tanstack/react-router";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { cacheSummary, garminProfile } from "../lib/api";
import { runSync } from "../lib/syncProgress";
import { since } from "../lib/format";
import { loadNavOrder, move, saveNavOrder, type NavEntry } from "../lib/nav";
import { Spring, rubberband } from "../lib/spring";
import { relaunch } from "@tauri-apps/plugin-process";
import { Swatch } from "./ui";
import { useUpdateState } from "./UpdateCheck";
import { useContextMenu } from "./ContextMenu";
import { useTheme } from "../lib/useTheme";
import {
  DarkIcon,
  LightIcon,
  MoveDownIcon,
  MoveUpIcon,
  PinIcon,
  SyncIcon,
  UpdateIcon,
} from "../lib/icons";

/**
 * The nav's width. Exported because the shell has to line the scroll fades up
 * with the nav's right edge — the nav is sticky, so it has nothing to fade.
 */
export const SIDEBAR_W = 228;

/** How far the pointer travels before a press becomes a drag. Under this a
 *  click is still a click, so pressing an entry and twitching still navigates. */
const THRESHOLD = 5;

/** A row getting out of the way wasn't thrown, so it doesn't overshoot. */
const SETTLE = { damping: 1, response: 0.32 };
/** A row that was, does — the flick is what makes the bounce read as physical
 *  rather than as decoration. */
const THROW = { damping: 0.8, response: 0.35 };

/**
 * Everything a drag needs. In a ref, not in state: it changes several times a
 * frame and none of it is worth a render.
 */
type Drag = {
  key: string;
  from: number;
  to: number;
  /** The order at grab time. Nothing commits until release, so this is what's
   *  on screen for the whole gesture. */
  order: NavEntry[];
  /** Where the rows rest on screen, measured once at the start. Everything
   *  after it is arithmetic and transform writes, so the gesture never asks the
   *  browser for geometry it has just invalidated. */
  tops: number[];
  centers: number[];
  min: number;
  max: number;
  startY: number;
  y: number;
  t: number;
  v: number;
  moved: boolean;
};

export function Sidebar() {
  const { toggle, next: nextTheme, label: themeLabel, preset, custom, paletteName } = useTheme();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const qc = useQueryClient();

  // The order is the user's, and the entry at the top of it is the screen the
  // app opens on. Held in state rather than read per render because a drag
  // rewrites it many times a second.
  const [nav, setNav] = useState<NavEntry[]>(loadNavOrder);
  const [pressed, setPressed] = useState<string | null>(null);
  const [drag, setDrag] = useState<{ key: string; to: number } | null>(null);
  const [announce, setAnnounce] = useState("");
  const menu = useContextMenu();

  const rowEls = useRef(new Map<string, HTMLDivElement>());
  const springs = useRef(new Map<string, Spring>());
  const gesture = useRef<Drag | null>(null);
  // A drag that ends over a link fires a click on it, which would navigate away
  // from wherever you were the moment you finished rearranging.
  const swallowClick = useRef(false);

  // Covers every route to a new order — dragging, the menu moves, the keyboard
  // moves, and the reconciliation `loadNavOrder` does on first mount when a
  // release has added a screen. localStorage is cheap enough not to matter.
  useEffect(() => {
    saveNavOrder(nav);
  }, [nav]);

  // The closed cursor belongs to the gesture, not to the row — the pointer is
  // captured, so it can be anywhere on screen while a row is still in hand. On
  // the root rather than in the row's own rules so the whole window agrees.
  const dragging = drag !== null;
  useEffect(() => {
    if (!dragging) return;
    document.documentElement.dataset.grabbing = "true";
    return () => {
      delete document.documentElement.dataset.grabbing;
    };
  }, [dragging]);

  const springFor = (key: string) => {
    let s = springs.current.get(key);
    if (!s) {
      s = new Spring((y) => {
        const el = rowEls.current.get(key);
        if (el) el.style.transform = y ? `translate3d(0,${y}px,0)` : "";
      });
      springs.current.set(key, s);
    }
    return s;
  };

  /**
   * Where a row sits on screen once everything has settled: its rect, with any
   * spring still in flight taken back out of it.
   *
   * Drawn position rather than `offsetTop`, because two things in this list are
   * drawn somewhere their layout doesn't know about — the default entry is
   * pinned, and everything under it can be scrolled. A gesture has to agree
   * with the screen, and after this so does the arithmetic: every use below is
   * a difference between two of these, so the scroll offset they share falls
   * straight back out.
   */
  const restingTop = (key: string) => {
    const el = rowEls.current.get(key);
    return el ? el.getBoundingClientRect().top - springFor(key).x : 0;
  };

  /**
   * Swap in a new order without anything appearing to teleport.
   *
   * Every row is read where it *looks* right now — transforms and mid-flight
   * springs included — then the order changes, then each row is offset back to
   * where it was and released. Starting from the presentation value rather than
   * the layout one is what lets a second move land on a row that is still
   * moving from the first.
   */
  const commit = (next: NavEntry[], thrown?: { key: string; v: number }) => {
    const before = new Map<string, number>();
    for (const [k, el] of rowEls.current) before.set(k, el.getBoundingClientRect().top);

    flushSync(() => setNav(next));

    // All the reads, then all the writes. Interleaved, each write would force
    // the next read to lay the list out again.
    const after = new Map<string, number>();
    for (const [k, el] of rowEls.current) {
      after.set(k, el.getBoundingClientRect().top - springFor(k).x);
    }
    for (const [k, was] of before) {
      const now = after.get(k);
      if (now === undefined) continue;
      const s = springFor(k);
      s.set(was - now);
      if (thrown?.key === k) s.to(0, thrown.v, THROW);
      else s.to(0, undefined, SETTLE);
    }
  };

  const reorder = (from: number, to: number) => {
    if (from < 0 || to < 0 || from === to || to >= nav.length) return;
    const { label } = nav[from];
    commit(move(nav, from, to));
    setAnnounce(
      to === 0
        ? `${label} moved to the top, and is now the screen the app opens on`
        : `${label} moved to position ${to + 1} of ${nav.length}`,
    );
  };

  /** Send every row that isn't under the finger to where a release would put it. */
  const displace = (d: Drag) => {
    const next = move(d.order, d.from, d.to);
    for (let j = 0; j < d.order.length; j++) {
      const n = d.order[j];
      if (n.to === d.key) continue;
      springFor(n.to).to(d.tops[next.indexOf(n)] - d.tops[j], undefined, SETTLE);
    }
  };

  const onPointerDown = (e: React.PointerEvent, i: number) => {
    if (e.button !== 0 || e.ctrlKey) return;
    swallowClick.current = false;
    // Feedback on the press, not on the release. Waiting for the click to show
    // anything is where directness falls off a cliff.
    setPressed(nav[i].to);
    gesture.current = {
      key: nav[i].to,
      from: i,
      to: i,
      order: nav,
      tops: [],
      centers: [],
      min: 0,
      max: 0,
      startY: e.clientY,
      y: e.clientY,
      t: e.timeStamp,
      v: 0,
      moved: false,
    };
  };

  const onPointerMove = (e: React.PointerEvent) => {
    const d = gesture.current;
    if (!d) return;
    const dy = e.clientY - d.startY;

    if (!d.moved) {
      if (Math.abs(dy) < THRESHOLD) return;
      const els = d.order.map((n) => rowEls.current.get(n.to));
      if (els.some((el) => !el)) return;
      d.tops = d.order.map((n) => restingTop(n.to));
      d.centers = d.tops.map((top, j) => top + els[j]!.offsetHeight / 2);
      d.min = d.tops[0] - d.tops[d.from];
      d.max = d.tops[d.order.length - 1] - d.tops[d.from];
      d.moved = true;
      // Captured only now. A plain click has to reach the anchor, and a pointer
      // captured on press would deliver it to this div instead.
      rowEls.current.get(d.key)?.setPointerCapture(e.pointerId);
      setPressed(null);
      setDrag({ key: d.key, to: d.from });
    }

    // The speed the release hands to the spring. Smoothed, because one
    // last-frame delta is noisy enough to throw a row the wrong way.
    const dt = (e.timeStamp - d.t) / 1000;
    if (dt > 0) d.v = 0.65 * ((e.clientY - d.y) / dt) + 0.35 * d.v;
    d.y = e.clientY;
    d.t = e.timeStamp;

    // Glued to the finger inside the list, resisting past either end. A hard
    // stop at the ends would read as the drag having broken.
    const span = Math.max(d.max - d.min, 1);
    const y =
      dy < d.min
        ? d.min - rubberband(d.min - dy, span)
        : dy > d.max
          ? d.max + rubberband(dy - d.max, span)
          : dy;
    springFor(d.key).set(y);

    // Where the row is now, against where the others rest. The list rearranges
    // as you pass over it rather than showing a drop line: the preview and the
    // result are then the same thing, so there's nothing to aim at and nothing
    // to mispredict.
    const center = d.centers[d.from] + y;
    let to = d.from;
    while (to > 0 && center < d.centers[to - 1]) to--;
    while (to < d.order.length - 1 && center > d.centers[to + 1]) to++;
    if (to !== d.to) {
      d.to = to;
      displace(d);
      setDrag({ key: d.key, to });
    }
  };

  const onPointerUp = () => {
    const d = gesture.current;
    gesture.current = null;
    setPressed(null);
    if (!d || !d.moved) return;
    setDrag(null);
    swallowClick.current = true;
    if (d.to === d.from) {
      // Picked up and put back. It still gets the throw, so letting go always
      // feels like letting go of something.
      springFor(d.key).to(0, d.v, THROW);
      return;
    }
    commit(move(d.order, d.from, d.to), { key: d.key, v: d.v });
    setAnnounce(
      d.to === 0
        ? `${d.order[d.from].label} moved to the top, and is now the screen the app opens on`
        : `${d.order[d.from].label} moved to position ${d.to + 1} of ${d.order.length}`,
    );
  };

  // The recent sync, not the full one — this is the button you press because
  // you just finished a session. A full re-sync stays in Settings, where its
  // cost is spelled out.
  const sync = useMutation({
    mutationFn: () => runSync(30, false),
    onSuccess: () => qc.invalidateQueries(),
  });

  // Both are cheap and cached; a stale name in the corner is not worth a
  // spinner, so neither blocks render.
  const profile = useQuery({
    queryKey: ["profile"],
    queryFn: garminProfile,
    staleTime: Infinity,
    retry: false,
  });
  const cache = useQuery({
    queryKey: ["cacheSummary"],
    queryFn: cacheSummary,
    refetchInterval: 30_000,
  });

  const update = useUpdateState();
  const who = profile.data?.fullName ?? profile.data?.displayName;

  // `startsWith`, so /activities/123 keeps the Activities entry lit.
  const activeKey = nav.find((n) => path.startsWith(n.to))?.to ?? null;

  // Mid-drag the word sits on whichever entry a release would leave on top, so
  // dragging something to the first slot tells you what that means before you
  // have committed to it. It's a word and not a box, so nothing reflows as it
  // moves between rows.
  const defaultKey = drag
    ? move(
        nav,
        nav.findIndex((n) => n.to === drag.key),
        drag.to,
      )[0].to
    : nav[0].to;

  // The lit entry's ground. One element, living inside whichever row is active,
  // so during a drag it rides that row's transform for free.
  const pill = useRef<HTMLSpanElement | null>(null);
  const pillSpring = useRef<Spring | null>(null);
  pillSpring.current ??= new Spring((y) => {
    if (pill.current) pill.current.style.transform = y ? `translate3d(0,${y}px,0)` : "";
  });
  const litKey = useRef<string | null>(null);
  // Navigating slides the ground from the old entry to the new one instead of
  // blinking it across, so the nav reads as one surface with a position on it
  // rather than as twelve independent lamps.
  useLayoutEffect(() => {
    const was = litKey.current;
    litKey.current = activeKey;
    if (!was || !activeKey || was === activeKey) return;
    if (!rowEls.current.has(was) || !rowEls.current.has(activeKey)) return;
    pillSpring.current!.set(restingTop(was) - restingTop(activeKey));
    pillSpring.current!.to(0, undefined, SETTLE);
  }, [activeKey]);

  /**
   * How much of the list fits, in whole entries.
   *
   * The height lands on some row's bottom edge, so the box can only ever end
   * where an entry does — half a row showing at the bottom reads as a rendering
   * fault rather than as "there is more here", and the scroll snapping in CSS
   * would still have left the bottom edge cutting one. Measured rather than
   * derived from a row height in the stylesheet, because the row's height
   * follows the typeface setting.
   *
   * `pin` and `pad` come out of the same measurement: where the divider rests,
   * which is where it pins, and where the first scrolling entry rests, which is
   * the offset a snapped row has to clear the pinned entry by.
   */
  const fit = useRef<HTMLDivElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState({ max: 0, full: 0, pad: 0, pin: 0 });
  useLayoutEffect(() => {
    const fitEl = fit.current;
    const listEl = list.current;
    const first = rowEls.current.get(nav[0].to);
    if (!fitEl || !listEl || !first) return;

    const measure = () => {
      const rows = nav.map((n) => rowEls.current.get(n.to));
      if (rows.some((el) => !el)) return;
      const bottom = (i: number) => rows[i]!.offsetTop + rows[i]!.offsetHeight;
      // Only ever asked for its height. The pinned row is the one element here
      // whose `offsetTop` lies: a stuck row reports the offset it's stuck by,
      // which is the scroll position — so measuring it while scrolled sent the
      // divider down the list by however far you'd scrolled, and the entries
      // passing under it into the gap that left. Its resting top is 0 anyway;
      // it's the first row of a list with nothing above it.
      const head = rows[0]!.offsetHeight;
      // Nothing above the first scrolling entry ever moves, so its resting top
      // is both the floor the list can shrink to and the padding a snap needs.
      const pad = rows.length > 1 ? rows[1]!.offsetTop : head;
      const room = fitEl.clientHeight;
      let max = pad;
      for (let i = 1; i < rows.length; i++) if (bottom(i) <= room) max = bottom(i);
      // A row's bottom is two rounded numbers added together and the whole
      // list's height is not, so a pixel short of it would hang a scrollbar on a
      // list that fits. When it does fit, the browser's own count is the answer
      // — and `max === full` is what tells the stylesheet not to scroll at all.
      const full = listEl.scrollHeight;
      if (full <= room) max = full;
      const next = { max, full, pad, pin: head + 2 };
      setBox((b) =>
        b.max === max && b.full === full && b.pad === pad && b.pin === next.pin ? b : next,
      );
    };

    measure();
    // The wrapper for the room available, the pinned row for the size of an
    // entry — which changes with the typeface, and with nothing else.
    const ro = new ResizeObserver(measure);
    ro.observe(fitEl);
    ro.observe(first);
    return () => ro.disconnect();
  }, [nav]);

  // Scrolling is a state the list is in, not something it always has: it clips,
  // it pins the default entry, and it takes a scrollbar, none of which a list
  // that fits has any use for.
  const scrolls = box.max > 0 && box.max < box.full;

  return (
    <nav
      className="sidebar"
      style={{
        width: SIDEBAR_W,
        flex: "none",
        padding: "38px 26px 40px 34px",
        position: "sticky",
        top: 0,
        height: "100vh",
        display: "flex",
        flexDirection: "column",
        gap: 2,
      }}
    >
      <div
        style={{
          font: "400 11px/1.4 'Instrument Sans', sans-serif",
          letterSpacing: "0.1em",
          textTransform: "uppercase",
          color: "var(--faint)",
          marginBottom: 26,
        }}
      >
        {who ?? ""}
      </div>

      <div className="nav-fit" ref={fit}>
        <div
          className="nav-list"
          ref={list}
          data-dragging={drag ? "true" : undefined}
          data-scrolls={scrolls || undefined}
          style={{
            maxHeight: scrolls ? box.max : undefined,
            scrollPaddingTop: box.pad || undefined,
            ["--nav-pin" as string]: `${box.pin}px`,
          }}
        >
          {nav.map((n, i) => {
            const active = n.to === activeKey;
            const Icon = n.icon;
            const isDefault = n.to === defaultKey;
            return (
              <Fragment key={n.to}>
                {/* Two layers on purpose: the row carries the drag's transform,
                    the link carries the press. One element would have them
                    overwriting each other. */}
                <div
                  className="nav-row"
                  data-dragging={drag?.key === n.to || undefined}
                  ref={(el) => {
                    if (el) rowEls.current.set(n.to, el);
                    else rowEls.current.delete(n.to);
                  }}
                  onPointerDown={(e) => onPointerDown(e, i)}
                  onPointerMove={onPointerMove}
                  onPointerUp={onPointerUp}
                  onPointerCancel={onPointerUp}
                >
                  <Link
                    to={n.to}
                    className="nav-link"
                    data-active={active}
                    data-pressed={pressed === n.to || undefined}
                    aria-current={active ? "page" : undefined}
                    title={
                      isDefault
                        ? "Opens when the app starts. Drag an entry to the top, or right-click, to change it."
                        : undefined
                    }
                    // The browser's own link dragging would fight the gesture for
                    // the pointer, and it has nothing to offer here.
                    onDragStart={(e) => e.preventDefault()}
                    onClick={(e) => {
                      if (!swallowClick.current) return;
                      swallowClick.current = false;
                      e.preventDefault();
                    }}
                    // Rearranging without a pointer. React keys the row by route,
                    // so the focused node survives the move and a run of presses
                    // walks an entry up the list the way holding a drag would.
                    onKeyDown={(e) => {
                      if (!e.altKey || e.metaKey || e.ctrlKey) return;
                      if (e.key === "ArrowUp") reorder(i, i - 1);
                      else if (e.key === "ArrowDown") reorder(i, i + 1);
                      else if (e.key === "Home") reorder(i, 0);
                      else return;
                      e.preventDefault();
                    }}
                    onContextMenu={(e) =>
                      menu.open(e, [
                        {
                          label: "Set as default screen",
                          icon: PinIcon,
                          disabled: i === 0,
                          onSelect: () => reorder(i, 0),
                        },
                        // The same rearranging as dragging, reachable without a
                        // pointer — and findable at all, which dragging isn't.
                        {
                          label: "Move up",
                          icon: MoveUpIcon,
                          divide: true,
                          disabled: i === 0,
                          onSelect: () => reorder(i, i - 1),
                        },
                        {
                          label: "Move down",
                          icon: MoveDownIcon,
                          disabled: i === nav.length - 1,
                          onSelect: () => reorder(i, i + 1),
                        },
                      ])
                    }
                  >
                    {active && <span className="nav-pill" ref={pill} aria-hidden />}
                    {/* Inherits the link's colour, so the icon lights with the label
                        on hover and on the active entry instead of needing its own
                        state. A step above the label's size, because the duotone
                        glyphs read a little smaller than their box. */}
                    <Icon size={17} style={{ flex: "none", position: "relative" }} aria-hidden />
                    <span className="nav-label">{n.label}</span>
                    {isDefault && <span className="nav-default">Default</span>}
                  </Link>
                </div>
                {i === 0 && <div className="nav-divide" />}
              </Fragment>
            );
          })}
        </div>
      </div>
      {menu.menu}

      {/* Reordering is silent otherwise: the list looks different, and a screen
          reader has been given no reason to say so. */}
      <span className="sr-only" aria-live="polite">
        {announce}
      </span>

      {/* The age sits on the button rather than under it: the question you
          have when looking at a Sync button is whether you still need to
          press it, and that reads better as one line than as two. */}
      <button
        className="quiet"
        onClick={() => sync.mutate()}
        disabled={sync.isPending}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          fontSize: 12.5,
          padding: "3.5px 0",
          color: sync.isPending ? "var(--faint)" : "var(--mut)",
          cursor: sync.isPending ? "default" : "pointer",
        }}
        title="Pull anything new from Garmin"
      >
        <SyncIcon
          size={14}
          className={sync.isPending ? "spin" : undefined}
          style={{ flex: "none" }}
          aria-hidden
        />
        {sync.isPending ? "Syncing…" : "Sync"}
        <span style={{ color: "var(--faint)" }}>
          {syncAge(sync.isPending, cache.data?.lastSync)}
        </span>
      </button>
      {sync.isError && (
        <div
          style={{ fontSize: 11.5, color: "var(--acc)", padding: "1px 0 3px", lineHeight: 1.35 }}
        >
          Sync failed. See Settings.
        </div>
      )}

      {/* An installed update should be findable from wherever you are, not
          only from the screen you'd have to think to open. */}
      {update.at === "ready" && (
        <button
          onClick={() => void relaunch()}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 12.5,
            padding: "3.5px 0",
            color: "var(--acc)",
          }}
          title={`Version ${update.version} is installed and starts on restart`}
        >
          <UpdateIcon size={14} style={{ flex: "none" }} aria-hidden />
          Restart to update
        </button>
      )}

      {/* A palette settles light and dark for itself, so there's nothing left
          here to flip. Rather than leave a control that does nothing — or one
          that quietly throws the palette away to change the lighting — the row
          becomes a statement of which palette is on, and the way back to where
          it was chosen. Same geometry either way, so the strip doesn't move. */}
      {paletteName ? (
        <Link
          to="/settings"
          className="quiet"
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            fontSize: 12.5,
            padding: "3.5px 0",
          }}
          title={`${paletteName} — light and dark are set in Settings`}
        >
          <Swatch of={preset ? preset.id : custom!} small />
          {paletteName}
        </Link>
      ) : (
        <button
          className="quiet"
          onClick={toggle}
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 12.5,
            padding: "3.5px 0",
          }}
          title="Light and dark only — 'match system' is in Settings"
        >
          {/* The icon shows the theme you'd get, matching the label, which reads
              as the destination rather than the current state. */}
          {nextTheme === "dark" ? (
            <DarkIcon size={14} style={{ flex: "none" }} aria-hidden />
          ) : (
            <LightIcon size={14} style={{ flex: "none" }} aria-hidden />
          )}
          {themeLabel}
        </button>
      )}
    </nav>
  );
}

/** " (2 minutes ago)", or nothing when there's no sync to describe. */
function syncAge(pending: boolean, lastSync: string | null | undefined): string {
  if (pending || !lastSync) return "";
  return ` (${since(lastSync)})`;
}
