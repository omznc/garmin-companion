/**
 * Turning a card into a PNG, and the PNG into someone else's screen.
 *
 * Two steps, and they fail for unrelated reasons, so they're separate calls:
 * `render` is the webview drawing pixels, `deliver` is the platform deciding
 * what "share" means. The button in `Share.tsx` reports them differently.
 *
 * The rasterising is `html-to-image`, which clones the node into an SVG
 * `foreignObject` with every computed style copied onto it and the web fonts
 * inlined as data URIs. Inlining the fonts is the part that matters here: the
 * app's face is Instrument Serif and a card that silently fell back to Georgia
 * would be worse than no card. This is also the step with the most platform
 * risk — Android's WebView is Chromium and does it without complaint, WebKitGTK
 * is the one that needed checking.
 */
import { toPng } from "html-to-image";
import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import {
  CARD,
  FIT_ATTR,
  SCALE,
  ShareCard,
  type ShareContent,
  type Shape,
} from "../components/ShareCard";
import { shareFile } from "./android";
import { IS_MOBILE } from "./platform";

export type { ShareContent, ShareMetric } from "../components/ShareCard";

/**
 * The shape this platform shares in. A phone always gets 9:16 and a desktop
 * always gets a square — neither is a preference, they're the shapes the two
 * destinations don't crop.
 */
export const SHAPE: Shape = IS_MOBILE ? "portrait" : "square";

export interface Shared {
  path: string;
  clipboard: boolean;
}

/**
 * Waits for the off-screen card to be worth photographing.
 *
 * `fonts.ready` because the whole point is the type, and two frames because
 * React 19 renders concurrently — the node exists after `render` returns but
 * has not necessarily been laid out, and `toPng` reads geometry.
 */
async function settle(): Promise<void> {
  await document.fonts.ready;
  await new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

/**
 * Shrinks the card's body if it doesn't fit the frame, and reports whether it
 * had to. See `FIT_ATTR` for why this exists rather than the layout simply
 * being made to fit.
 *
 * Scaling rather than dropping content: what overflows is usually one row of
 * one block, and losing a whole metric to save 14px is a worse card than one
 * drawn a few percent smaller. There's a floor, though — past it the card is
 * illegible at thumbnail size, and clipping the bottom is the better failure.
 */
function fitToBox(stage: HTMLElement): boolean {
  const fit = stage.querySelector<HTMLElement>(`[${FIT_ATTR}]`);
  const box = fit?.parentElement;
  if (!fit || !box) return false;

  // The content box, not `clientHeight` — that includes the box's own padding,
  // and measuring against it is how a block 14px too tall was declared to fit
  // and then had its zone legend sliced off by the overflow rule.
  //
  // Then a few pixels of slack on top, because scaling to the exact height
  // lands the last line's descenders on the boundary where sub-pixel rounding
  // still shaves them.
  const style = getComputedStyle(box);
  const available =
    box.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom) - 6;
  const needed = fit.offsetHeight;
  if (needed <= available || needed <= 0 || available <= 0) return false;

  const factor = Math.max(available / needed, 0.72);

  // Widened by exactly as much as it's about to shrink, so the block still
  // reaches the right margin afterwards. Without this the scale reads as the
  // content being narrow rather than smaller: the chart stops short of the
  // edge, the right margin comes out half again as wide as the left, and the
  // card looks mislaid out instead of merely tighter. Scaling from the left
  // edge is what makes the two cancel.
  fit.style.width = `${100 / factor}%`;
  fit.style.transform = `scale(${factor})`;
  return true;
}

/**
 * Draws the card off-screen and returns it as bare base64 PNG.
 *
 * Off-screen rather than in a detached node, because a detached node inherits
 * no CSS variables and the card would come out unstyled. `left: -10000px` keeps
 * it out of the way while leaving it in the cascade; `aria-hidden` and
 * `pointer-events: none` keep it out of everything else.
 *
 * The two nested elements are not decoration. `html-to-image` clones the node
 * it's given and copies that node's own computed style onto the clone, then
 * lays the clone out inside an SVG `foreignObject` at the origin. Hand it the
 * off-screen host and the clone keeps `position: fixed; left: -10000px`, which
 * inside a 540px viewport means the card renders ten thousand pixels to the
 * left of the frame: a correctly-sized PNG of the background colour and nothing
 * else. The stage is the same box with no positioning of its own, so it's the
 * one that gets photographed.
 */
export async function renderCard(content: ShareContent): Promise<string> {
  const { width, height } = CARD[SHAPE];

  const host = document.createElement("div");
  host.setAttribute("aria-hidden", "true");
  Object.assign(host.style, {
    position: "fixed",
    left: "-10000px",
    top: "0",
    pointerEvents: "none",
  });

  const stage = document.createElement("div");
  Object.assign(stage.style, { width: `${width}px`, height: `${height}px` });
  host.appendChild(stage);
  document.body.appendChild(host);

  const root = createRoot(stage);
  try {
    root.render(<ShareCard content={content} shape={SHAPE} />);
    await settle();
    if (fitToBox(stage)) await settle();

    const dataUrl = await toPng(stage, {
      width,
      height,
      pixelRatio: SCALE,
      // The card paints its own background, but an explicit one here means a
      // failure to clone that node can't produce a transparent PNG.
      backgroundColor: getComputedStyle(document.documentElement).getPropertyValue("--bg").trim(),
      // Everything drawn is same-origin and immutable per build, and the
      // cache-busting query would only defeat the font cache between cards.
      cacheBust: false,
    });

    const comma = dataUrl.indexOf(",");
    if (!dataUrl.startsWith("data:image/png") || comma < 0) {
      throw new Error("the card didn't render");
    }
    return dataUrl.slice(comma + 1);
  } finally {
    // Unmount before removing the host, so React doesn't hold a root over a
    // node that has left the document.
    root.unmount();
    host.remove();
  }
}

/**
 * Hands the PNG to the platform.
 *
 * Rust writes the file either way — see `share.rs` — and then the two diverge:
 * a desktop has the image on its clipboard and a copy in Pictures, while
 * Android has a path that only means anything to the sharesheet, which is
 * opened here through the same JS bridge the installer uses.
 */
export async function deliverCard(png: string, name: string): Promise<Shared> {
  const shared = await invoke<Shared>("share_image", { png, name });
  if (IS_MOBILE) shareFile(shared.path);
  return shared;
}
