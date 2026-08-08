/**
 * Custom window chrome. The window is built with `decorations: false`, so the
 * top strip of the webview has to provide both the drag handle and the
 * minimise/maximise/close controls the compositor would otherwise draw.
 *
 * The strip is a fixed overlay rather than a row in the layout: the sidebar is
 * meant to run unbroken to the top edge, and reserving a band for a title bar
 * would cut it in half. Its height matches the sidebar's top padding, so it
 * covers only empty space and never swallows a click meant for the app.
 */
import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { CloseIcon, MaximiseIcon, MinimiseIcon, RestoreIcon } from "../lib/icons";
import { IS_MAC } from "../lib/platform";

/** Mirrors the API's own union, which it declares but doesn't export. */
type ResizeDirection =
  | "North"
  | "South"
  | "East"
  | "West"
  | "NorthEast"
  | "NorthWest"
  | "SouthEast"
  | "SouthWest";

/** Matches the sidebar's 38px top padding — see the note above. */
export const STRIP = 38;

/**
 * Which end of the strip the window controls sit at. macOS puts them top-left
 * and everything else top-right, and that is the one difference the rest of the
 * app has to care about — anything else drawing in the strip has to know which
 * end is already spoken for.
 */
export const CONTROLS_SIDE = IS_MAC ? "left" : "right";

/**
 * How much of that end they occupy. On macOS three 12px dots, the 8px gaps
 * between them and the 20px inset the system uses; elsewhere three 26px
 * buttons, 2px gaps and a 10px inset.
 */
export const CONTROLS_W = IS_MAC ? 20 + 3 * 12 + 2 * 8 : 10 + 3 * 26 + 2 * 2;

/**
 * Resize handles.
 *
 * An undecorated window gets no resize border from the compositor, so without
 * these the window is stuck at its launch size. Each handle hands the gesture
 * straight back to the window manager via `startResizeDragging`, which keeps
 * the snapping and constraint behaviour a native border would have.
 */
// Generous on purpose. A 4–5px border is what a compositor draws, but it also
// gets a few pixels of slop outside the window to catch the pointer, which an
// in-window handle does not — so the whole target has to live on this side of
// the edge to be grabbable at all.
const EDGE = 10;
const CORNER = 26;
// The bottom-right corner is the one people actually aim for, so it gets a
// bigger target than the other three and a visible grip to aim at. It only
// works because the document scrollbar is hidden (see `styles.css`) — a native
// scrollbar hit-tests over this area and swallows the gesture.
const SE_CORNER = 34;

const HANDLES: {
  dir: ResizeDirection;
  cursor: string;
  className?: string;
  style: React.CSSProperties;
}[] = [
  {
    dir: "North",
    cursor: "ns-resize",
    style: { top: 0, left: CORNER, right: CORNER, height: EDGE },
  },
  {
    dir: "South",
    cursor: "ns-resize",
    style: { bottom: 0, left: CORNER, right: SE_CORNER, height: EDGE },
  },
  {
    dir: "West",
    cursor: "ew-resize",
    style: { left: 0, top: CORNER, bottom: CORNER, width: EDGE },
  },
  {
    dir: "East",
    cursor: "ew-resize",
    style: { right: 0, top: CORNER, bottom: SE_CORNER, width: EDGE },
  },
  {
    dir: "NorthWest",
    cursor: "nwse-resize",
    style: { top: 0, left: 0, width: CORNER, height: CORNER },
  },
  {
    dir: "NorthEast",
    cursor: "nesw-resize",
    style: { top: 0, right: 0, width: CORNER, height: CORNER },
  },
  {
    dir: "SouthWest",
    cursor: "nesw-resize",
    style: { bottom: 0, left: 0, width: CORNER, height: CORNER },
  },
  {
    dir: "SouthEast",
    cursor: "nwse-resize",
    className: "resize-grip",
    style: { bottom: 0, right: 0, width: SE_CORNER, height: SE_CORNER },
  },
];

function ResizeHandles() {
  return (
    <>
      {HANDLES.map((h) => (
        <div
          key={h.dir}
          className={h.className}
          // Pointer down rather than click: the drag has to start while the
          // button is still held, or the window manager gets nothing to track.
          onPointerDown={(e) => {
            if (e.button !== 0) return;
            e.preventDefault();
            void getCurrentWindow().startResizeDragging(h.dir);
          }}
          style={{
            position: "fixed",
            zIndex: 60,
            cursor: h.cursor,
            ...h.style,
          }}
        />
      ))}
    </>
  );
}

export function WindowChrome() {
  const [maximized, setMaximized] = useState(false);

  // The window can be maximised by the compositor (keyboard shortcut, snap,
  // double-click on the drag region) with no click of ours involved, so the
  // icon follows the window rather than our own button presses.
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    let stale = false;

    win.isMaximized().then((m) => !stale && setMaximized(m));
    win
      .onResized(() => win.isMaximized().then((m) => !stale && setMaximized(m)))
      .then((fn) => {
        if (stale) fn();
        else unlisten = fn;
      });

    return () => {
      stale = true;
      unlisten?.();
    };
  }, []);

  // Published to CSS so the corner radius can flatten while maximised. A
  // maximised window is flush with the screen edges on every platform, and a
  // rounded corner there leaves a notch of desktop showing through.
  useEffect(() => {
    const root = document.documentElement;
    if (maximized) root.dataset.maximized = "true";
    else delete root.dataset.maximized;
  }, [maximized]);

  const win = getCurrentWindow();

  // The glyphs only show at all on macOS while the group is hovered, and they
  // sit inside a 12px dot rather than a 26px button, so they're drawn smaller
  // there. `bold` rather than the app's duotone on both: a window control is
  // two or three strokes, and duotone's tint layer has nothing to fill in a
  // minus or a cross — it only makes the strokes lighter than they deserve.
  const glyph = IS_MAC ? 8 : 13;

  const minimise = (
    <button
      key="minimise"
      className="win-btn win-btn-minimise"
      onClick={() => win.minimize()}
      title="Minimise"
      aria-label="Minimise"
    >
      <MinimiseIcon size={glyph} weight="bold" aria-hidden />
    </button>
  );

  // macOS calls this one Zoom and calls it that in both directions — there is no
  // separate "restore" in its vocabulary, so only the icon changes there.
  const zoomLabel = IS_MAC ? "Zoom" : maximized ? "Restore" : "Maximise";
  const maximise = (
    <button
      key="maximise"
      className="win-btn win-btn-maximise"
      onClick={() => win.toggleMaximize()}
      title={zoomLabel}
      aria-label={zoomLabel}
    >
      {maximized ? (
        <RestoreIcon size={glyph} weight="bold" aria-hidden />
      ) : (
        <MaximiseIcon size={IS_MAC ? glyph : 12} weight="bold" aria-hidden />
      )}
    </button>
  );

  const close = (
    <button
      key="close"
      className="win-btn win-btn-close"
      onClick={() => win.close()}
      title="Close"
      aria-label="Close"
    >
      <CloseIcon size={glyph} weight="bold" aria-hidden />
    </button>
  );

  return (
    <>
      <ResizeHandles />
      <div data-tauri-drag-region className="drag-strip" style={{ height: STRIP }} />
      {/* Order is the platform's, not ours: macOS reads close, minimise, zoom
          from the left, everyone else minimise, maximise, close from the left.
          Reversing the array would put close under the pointer where minimise
          belongs, which is the one mistake in a title bar that costs work. */}
      <div className="win-controls" style={{ height: STRIP }}>
        {IS_MAC ? [close, minimise, maximise] : [minimise, maximise, close]}
      </div>
    </>
  );
}
