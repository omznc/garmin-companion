/**
 * Which desktop this build is running on.
 *
 * The window is undecorated on every platform, so the app draws its own title
 * bar — and the conventions for one differ enough between platforms that a
 * single layout is wrong on at least two of them. This is the one place that
 * answers "which", and everything platform-shaped reads it from here.
 *
 * `platform()` is a compile-time constant handed to the webview at startup, not
 * an IPC call, so the answer is available on the first line of the app. That
 * matters: an async answer would render the controls on one side and then move
 * them, which is worse than having them on the wrong side to begin with.
 *
 * Note that the window config is split to match — `tauri.macos.conf.json` and
 * `tauri.linux.conf.json` mark the window transparent so the corners below can
 * be clipped when there's something behind them to clip to (see
 * `COMPOSITES_ALPHA`), while Windows stays opaque and lets the compositor round
 * it. Those
 * files restate the whole window object rather than adding one key, because
 * Tauri merges platform config as an RFC 7396 patch and arrays are replaced
 * wholesale, not merged. Any change to the base window block has to be made in
 * all three.
 */
import { platform } from "@tauri-apps/plugin-os";

export type OSName = "macos" | "windows" | "linux" | "android";

/**
 * Everything Tauri doesn't build for — BSDs and Solaris are all in `platform()`'s
 * union — is treated as Linux, since that's the desktop stack they run.
 */
function detect(): OSName {
  try {
    const p = platform();
    if (p === "macos") return "macos";
    if (p === "windows") return "windows";
    if (p === "android") return "android";
    return "linux";
  } catch {
    // The plugin's constant is injected by the Tauri runtime, so it isn't there
    // when the dev server is opened in a plain browser. Guessing from the user
    // agent keeps that case working rather than crashing before first paint;
    // it is never the path a packaged build takes.
    const ua = navigator.userAgent;
    // Before the Mac test: an Android UA string contains "Linux", not "Mac",
    // but Chrome on Android has said "like Mac OS X" in the past and the order
    // costs nothing to get right.
    if (ua.includes("Android")) return "android";
    if (ua.includes("Mac")) return "macos";
    if (ua.includes("Windows")) return "windows";
    return "linux";
  }
}

export const OS = detect();
export const IS_MAC = OS === "macos";

/**
 * Whether this is a phone.
 *
 * The one flag the layout branches on, and it asks the platform rather than
 * measuring the viewport. A narrow desktop window is still a desktop window: it
 * has a pointer, a title bar to draw, and a nav the user can reorder by
 * dragging. A phone has none of those at any width, and most of what differs —
 * safe-area insets, tap targets, no hover, no window chrome — follows from the
 * input device and the shell rather than from how much room there is.
 *
 * Where the amount of room is genuinely the question, `styles.css` uses a width
 * media query and this has nothing to say about it.
 */
export const IS_MOBILE = OS === "android";

/**
 * What to call the place credentials are kept, in a sentence.
 *
 * The app tells people where their Garmin token lives, in three places, and the
 * answer is genuinely different on a phone — Android has no keyring, so
 * `garmin-core::secrets` writes an encrypted file into the app's private
 * directory instead. Telling an Android user to look in their OS keyring would
 * be pointing at something that doesn't exist.
 *
 * Mirrors `secrets::STORE` on the Rust side, which does the same job for error
 * messages. If one changes, change both.
 */
export const STORE = IS_MOBILE ? "this app's encrypted store" : "your OS keyring";

/**
 * Whether a transparent pixel in this window shows the desktop behind it.
 *
 * The corners below are cut out of the window rather than drawn on it, so they
 * only read as corners if something composites what's behind the cut. Where
 * nothing does, the same cut is a hole with undefined pixels in it — which is
 * what a Linux AppImage forced onto XWayland used to look like.
 *
 * Set by the `surface` plugin's init script (see `lib.rs`), so it's here before
 * the first line of the app runs, same as `platform()`. Missing means a plain
 * browser on the dev server, where the page has no window surface of its own
 * and square is the honest answer.
 */
export const COMPOSITES_ALPHA: boolean =
  (window as { __GARMIN_COMPOSITES_ALPHA__?: boolean }).__GARMIN_COMPOSITES_ALPHA__ ?? false;

/**
 * Publishes the platform to CSS. Called before the first render, so the rules
 * keyed on `[data-os]` — the corner radius, the shape of the window controls —
 * are in force for the first paint rather than applied a frame later.
 *
 * `data-mobile` rides along so a rule that applies to every phone doesn't have
 * to name each mobile OS, and won't need revisiting when iOS is added.
 */
export function applyPlatform() {
  const root = document.documentElement;
  root.dataset.os = OS;
  if (IS_MOBILE) root.dataset.mobile = "true";
  if (COMPOSITES_ALPHA) root.dataset.composited = "true";
  else delete root.dataset.composited;
}
