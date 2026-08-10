/**
 * The share control, and the row it shares with refresh.
 *
 * State is inline in the label, the way `RefreshButton` does it, because this
 * app has no toast and shouldn't grow one for this: the button is the thing you
 * just pressed and the thing you're looking at, and a notification sliding in
 * from a corner to report on it would be a second place to look.
 *
 * What the label can honestly claim differs by platform, and that's decided by
 * what came back from Rust rather than guessed here — a desktop with no
 * reachable clipboard says "Saved", not "Copied".
 */
import { useEffect, useRef, useState } from "react";
import { deliverCard, renderCard, type ShareContent } from "../lib/share";
import { IS_MOBILE } from "../lib/platform";
import { RefreshButton } from "./Refresh";

/** How long the outcome stays in the label before it goes back to "Share". */
const SETTLE_MS = 4000;

type State =
  | { kind: "idle" }
  | { kind: "working" }
  | { kind: "done"; label: string; title: string }
  | { kind: "failed"; title: string };

export function ShareButton({
  content,
  /**
   * The filename, and nothing else — the card carries its own title. Slugged on
   * the Rust side, so a screen can pass whatever it calls itself.
   */
  name,
}: {
  /** Built lazily: a screen shouldn't compose a card nobody asked for. */
  content: () => ShareContent;
  name: string;
}) {
  const [state, setState] = useState<State>({ kind: "idle" });
  const timer = useRef<number | undefined>(undefined);

  // The work outlives a navigation away from the screen — rendering is a
  // couple of hundred milliseconds and the sharesheet is longer — so the
  // timeout has to be cleared or it sets state on an unmounted button.
  useEffect(() => () => window.clearTimeout(timer.current), []);

  const settle = (next: State) => {
    setState(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setState({ kind: "idle" }), SETTLE_MS);
  };

  const run = async () => {
    if (state.kind === "working") return;
    setState({ kind: "working" });
    try {
      const png = await renderCard(content());
      const shared = await deliverCard(png, name);

      if (IS_MOBILE) {
        // The sheet is up; which app it lands in is not ours to know.
        settle({ kind: "done", label: "Shared", title: "Sent to the sharesheet" });
      } else if (shared.clipboard) {
        settle({
          kind: "done",
          label: "Copied",
          title: `On the clipboard, and saved to ${shared.path}`,
        });
      } else {
        settle({ kind: "done", label: "Saved", title: `Saved to ${shared.path}` });
      }
    } catch (e) {
      settle({ kind: "failed", title: e instanceof Error ? e.message : String(e) });
    }
  };

  const label =
    state.kind === "working"
      ? "Rendering…"
      : state.kind === "done"
        ? state.label
        : state.kind === "failed"
          ? "Didn't share"
          : "Share";

  return (
    <button
      className="quiet"
      onClick={run}
      disabled={state.kind === "working"}
      title={
        state.kind === "done" || state.kind === "failed"
          ? state.title
          : IS_MOBILE
            ? "Make an image of this screen and share it"
            : "Make an image of this screen and copy it"
      }
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 7,
        fontSize: "var(--fs-caption)",
        letterSpacing: "0.02em",
        // Same reasoning as the refresh button beside it: the outcome is said
        // in words rather than coloured in, so accent keeps meaning one thing.
        color: state.kind === "working" ? "var(--faint)" : "var(--mut)",
        cursor: state.kind === "working" ? "default" : "pointer",
        flex: "none",
      }}
    >
      <ShareArrow />
      {label}
    </button>
  );
}

/** Box with an arrow leaving the top of it, at the hairline weight everything
 *  else in this row is drawn at. */
function ShareArrow() {
  return (
    <svg
      width="13"
      height="13"
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8 10V2" />
      <path d="M5 5l3-3 3 3" />
      <path d="M3.5 8.5v5h9v-5" />
    </svg>
  );
}

/**
 * The pair, in the order they go in a page header.
 *
 * Refresh is the one that's always there and the one people reach for, so it
 * keeps the outside position it has on every other screen; share sits inboard
 * of it rather than displacing it.
 */
export function ScreenActions({
  share,
  name,
  days,
  live,
}: {
  share: () => ShareContent;
  name: string;
  days?: number;
  live?: boolean;
}) {
  return (
    <div style={{ display: "inline-flex", alignItems: "center", gap: 18, flex: "none" }}>
      <ShareButton content={share} name={name} />
      <RefreshButton days={days} live={live} />
    </div>
  );
}
