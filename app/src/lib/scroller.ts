/**
 * The element that scrolls.
 *
 * Not the document, which is what a web page would use. On the platforms where
 * the app rounds its own window corners the window surface is transparent and
 * the rounding is a `border-radius` in CSS — and a radius only clips what is
 * inside the box that carries it. The document can't be that box: giving `html`
 * or `body` the radius either fails to clip (the root's background propagates to
 * the whole canvas, corners included) or takes the scroll with it.
 *
 * So `#root` carries the background, the radius and the overflow, and is
 * therefore what scrolls. `window.scrollY` and `window.scrollTo` no longer refer
 * to anything on this page; go through here instead.
 */
export function scroller(): HTMLElement {
  // Present before React mounts — `main.tsx` renders into it — so this is safe
  // from any effect or event handler in the tree.
  return document.getElementById("root")!;
}
