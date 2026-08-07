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
const STRIP = 38;

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
const EDGE = 9;
const CORNER = 24;

const HANDLES: { dir: ResizeDirection; cursor: string; style: React.CSSProperties }[] = [
  { dir: "North", cursor: "ns-resize", style: { top: 0, left: CORNER, right: CORNER, height: EDGE } },
  { dir: "South", cursor: "ns-resize", style: { bottom: 0, left: CORNER, right: CORNER, height: EDGE } },
  { dir: "West", cursor: "ew-resize", style: { left: 0, top: CORNER, bottom: CORNER, width: EDGE } },
  { dir: "East", cursor: "ew-resize", style: { right: 0, top: CORNER, bottom: CORNER, width: EDGE } },
  { dir: "NorthWest", cursor: "nwse-resize", style: { top: 0, left: 0, width: CORNER, height: CORNER } },
  { dir: "NorthEast", cursor: "nesw-resize", style: { top: 0, right: 0, width: CORNER, height: CORNER } },
  { dir: "SouthWest", cursor: "nesw-resize", style: { bottom: 0, left: 0, width: CORNER, height: CORNER } },
  { dir: "SouthEast", cursor: "nwse-resize", style: { bottom: 0, right: 0, width: CORNER, height: CORNER } },
];

function ResizeHandles() {
  return (
    <>
      {HANDLES.map((h) => (
        <div
          key={h.dir}
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

  const win = getCurrentWindow();

  return (
    <>
      <ResizeHandles />
      <div data-tauri-drag-region className="drag-strip" style={{ height: STRIP }} />
      <div className="win-controls" style={{ height: STRIP }}>
        <button className="win-btn" onClick={() => win.minimize()} title="Minimise" aria-label="Minimise">
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M1 5h8" />
          </svg>
        </button>
        <button
          className="win-btn"
          onClick={() => win.toggleMaximize()}
          title={maximized ? "Restore" : "Maximise"}
          aria-label={maximized ? "Restore" : "Maximise"}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            {maximized ? (
              <path d="M3 1.6h5.4V7M1.6 3h5.4v5.4H1.6z" />
            ) : (
              <path d="M1.6 1.6h6.8v6.8H1.6z" />
            )}
          </svg>
        </button>
        <button
          className="win-btn win-btn-close"
          onClick={() => win.close()}
          title="Close"
          aria-label="Close"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M1.6 1.6l6.8 6.8M8.4 1.6l-6.8 6.8" />
          </svg>
        </button>
      </div>
    </>
  );
}
