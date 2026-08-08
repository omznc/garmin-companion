/**
 * The page fading out where it runs past the top and bottom of the window.
 *
 * The window has no title bar and no scrollbar, so there is nothing at either
 * edge to say the page continues past it — text simply stops mid-line at a
 * hard boundary. A short gradient in the page's own colour turns that cut into
 * a fade, and because each end only appears once there is something in that
 * direction to scroll to, the pair doubles as the scroll position readout the
 * hidden scrollbar took away.
 *
 * Two fixed overlays rather than a mask on the scroller: masking it would take
 * the sticky sidebar and the window chrome down with it. They sit below every
 * part of the chrome (z 50–70) so the sync readout and the window controls stay
 * crisp, and take no pointer events, so nothing underneath becomes unclickable.
 *
 * `left` is the column the fade covers — the desktop shell passes the sidebar's
 * right edge, since the nav is sticky and has nothing to fade; the phone has no
 * nav beside the column and passes 0.
 *
 * On a phone the edges are different edges, and `styles.css` moves them there.
 * Android draws the status bar over the webview rather than above it, and the
 * tab bar covers the bottom of the screen — so what content actually disappears
 * behind is the status bar's lower lip and the tab bar's top hairline, not the
 * screen edges.
 */
import { useEffect, useRef, type RefObject } from "react";
import { createPortal } from "react-dom";
import { scroller } from "../lib/scroller";
import { IS_MOBILE } from "../lib/platform";

/**
 * How far you have to scroll away from an edge before that edge's fade reaches
 * full strength. Short enough to have arrived by the time you notice you've
 * scrolled, long enough that it grows in rather than snapping on.
 */
const RAMP = 28;

const clamp = (n: number) => Math.max(0, Math.min(1, n));

export function ScrollFade({
  left,
  track,
}: {
  left: string;
  /** The box inside the scroller that grows with the page. See the observer
   *  below for what it's for and why the fade can't find it on its own. */
  track: RefObject<HTMLElement | null>;
}) {
  const top = useRef<HTMLDivElement>(null);
  const bottom = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let queued = 0;
    const box = scroller();

    const read = () => {
      queued = 0;
      const above = box.scrollTop;
      const below = box.scrollHeight - box.clientHeight - above;
      // Clamped because an overscroll bounce puts both of these out of range,
      // and a page shorter than the window makes `below` negative.
      if (top.current) top.current.style.opacity = String(clamp(above / RAMP));
      if (bottom.current) bottom.current.style.opacity = String(clamp(below / RAMP));
    };

    // Written straight to the nodes and coalesced into a frame rather than held
    // in state: this runs on every scroll event, the reads above flush layout,
    // and re-rendering the tree to move two opacities would be the expensive
    // part of scrolling the app.
    const schedule = () => {
      if (!queued) queued = requestAnimationFrame(read);
    };

    read();
    box.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    // Whether there is a bottom edge to fade at all is a question about the
    // page's length, which changes with no scroll and no resize every time a
    // screen swaps or a query resolves.
    //
    // The shell is what's watched. Not the scroller, which is pinned to the
    // window's height and so only ever changes on resize; and not
    // `document.body`, which since the scroll moved off the document is pinned
    // to the window's height too. The shell is the box inside the scroller that
    // actually grows with the page.
    //
    // Handed in rather than reached through the fade's own parent, because on a
    // phone the fade is not inside the shell at all — see the portal below.
    const ro = new ResizeObserver(schedule);
    const shell = track.current;
    if (shell) ro.observe(shell);

    return () => {
      if (queued) cancelAnimationFrame(queued);
      box.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      ro.disconnect();
    };
  }, [track]);

  const fades = (
    <>
      <div ref={top} className="scroll-fade scroll-fade-top" style={{ left }} aria-hidden />
      <div ref={bottom} className="scroll-fade scroll-fade-bottom" style={{ left }} aria-hidden />
    </>
  );

  // Out to the body on a phone, for the reason `TabBar` and `SyncBar` do it:
  // Android's overscroll stretches the whole scrolling layer, and anything
  // `position: fixed` inside the scroller stretches with it. A fade that slid
  // off its own edge on every rubber-band would be worse than no fade.
  return IS_MOBILE ? createPortal(fades, document.body) : fades;
}
