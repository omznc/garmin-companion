/**
 * The narrated sync.
 *
 * Lives in the window's top strip, in the empty run between the drag handle
 * and the window controls, because the sync is started from two places and
 * outlives the screen you started it on. It says which stage is running, which
 * date it's fetching, and — where the stage knows its own length — how far
 * along it is.
 *
 * The strip is space the app already reserves and never draws in, so the
 * readout costs the page nothing: no row is covered and no layout moves when a
 * sync starts. It takes no pointer events either, so the whole width of it
 * stays a drag handle for the window.
 *
 * # On a phone it goes to the other end
 *
 * There is no strip to draw in: the window has no title bar, and the band where
 * one would be is underneath Android's status bar — so the first version of
 * this put a sync readout behind the clock and the battery icon, in the app's
 * own faint grey, which is close to invisible.
 *
 * So it moves to the bottom, above the tab bar, and stops being a bare line of
 * text: down there it has the page behind it rather than empty chrome, so it
 * needs a surface of its own to be legible against. That makes it a card rather
 * than a strip, which is what a phone would have done with a background task in
 * the first place.
 */
import { useRef, useSyncExternalStore } from "react";
import { createPortal } from "react-dom";
import { describe, fraction, getSyncState, subscribe, type SyncStep } from "../lib/syncProgress";
import { IS_MOBILE } from "../lib/platform";
import { CONTROLS_SIDE, CONTROLS_W, STRIP } from "./WindowChrome";

export function useSyncState() {
  return useSyncExternalStore(subscribe, getSyncState);
}

export function SyncBar() {
  const sync = useSyncState();

  // The store clears `step` in the same update as `running`, and `describe(null)`
  // is a sync's *opening* line — so a readout that kept reading the live step
  // would flip to "Starting" for the length of its own fade-out. The last step
  // it showed is held instead, and the strip leaves saying what it last did.
  const last = useRef<SyncStep | null>(null);
  if (sync.running) last.current = sync.step;
  const step = sync.running ? sync.step : last.current;

  const { title, detail } = describe(step);
  const pct = fraction(step);

  const bar = (
    <div
      role="status"
      // Hidden from the tree while faded out — it is still on the page, and a
      // screen reader should not find a stale sync report in it.
      aria-hidden={!sync.running}
      className="sync-bar"
      data-running={sync.running}
      data-mobile={IS_MOBILE || undefined}
      style={
        IS_MOBILE
          ? // Position and surface are in `styles.css` — it sits above the tab
            // bar, which means composing with `env(safe-area-inset-bottom)`,
            // and that is only readable from a stylesheet.
            undefined
          : {
              position: "fixed",
              top: 0,
              // Clear of the controls, and never so wide that it reaches the drag
              // strip's grab bar in the middle of a narrow window. On macOS the
              // controls are at the other end, so this end only needs the inset it
              // would have had anyway.
              right: CONTROLS_SIDE === "right" ? CONTROLS_W : 12,
              maxWidth: "min(42vw, 460px)",
              height: STRIP,
              display: "flex",
              alignItems: "center",
              gap: 9,
              // Over the drag strip and the corner resize handle, under the window
              // controls — the same order the controls themselves keep.
              zIndex: 60,
              // The readout is text, not a control. Handing the events straight
              // through keeps this part of the strip draggable.
              pointerEvents: "none",
            }
      }
    >
      <span className="pulse-dot" style={{ flex: "none" }} />

      <span style={{ fontSize: "var(--fs-small)", flex: "none" }}>{title}</span>
      {/* The one part that can be any length — a date, an activity name — so
          it is the one part allowed to shrink and clip. */}
      <span
        className="shimmer"
        style={{
          fontSize: "var(--fs-small)",
          // The counts inside the sentence tick over every few hundred ms.
          // Same-width digits keep the words around them still.
          fontVariantNumeric: "tabular-nums",
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {detail}
      </span>

      {/* A rail only where there's a real denominator — a bar that invents its
          own length is worse than no bar. */}
      {pct != null ? (
        <>
          <span
            style={{
              flex: "none",
              width: 56,
              height: 2,
              background: "var(--line)",
              borderRadius: 2,
              overflow: "hidden",
            }}
          >
            <span
              style={{
                display: "block",
                height: "100%",
                width: "100%",
                background: "var(--acc)",
                // Scaled rather than resized: a `width` transition relayouts
                // the strip on every frame of a sync that is already busy.
                transform: `scaleX(${pct})`,
                transformOrigin: "left",
                transition: "transform .3s linear",
              }}
            />
          </span>
          <span
            className="mono"
            style={{
              fontSize: "var(--fs-caption)",
              color: "var(--faint)",
              flex: "none",
              // Room for "100%" from the first frame, right-aligned. The strip
              // shrink-wraps its content against the right edge, so a readout
              // that grew from 2 characters to 4 would drag the whole line —
              // title, detail and rail — leftwards as the sync finished.
              width: "4ch",
              textAlign: "right",
            }}
          >
            {Math.round(pct * 100)}%
          </span>
        </>
      ) : (
        <span
          className="mono"
          style={{ fontSize: "var(--fs-caption)", color: "var(--faint)", flex: "none" }}
        >
          {sync.full ? "Full sync" : "Sync"}
        </span>
      )}
    </div>
  );

  // On a phone this floats over content that rubber-bands at the ends of a
  // scroll, and Android applies that stretch to the whole scrolling layer —
  // `position: fixed` inside it included. Out of `#root` it can't be caught by
  // it. Same reason `TabBar` portals; see the note there. On a desktop it draws
  // into the window strip and there is nothing to escape.
  return IS_MOBILE ? createPortal(bar, document.body) : bar;
}
