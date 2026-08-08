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
 * be clipped, while Windows stays opaque and lets the compositor round it. Those
 * files restate the whole window object rather than adding one key, because
 * Tauri merges platform config as an RFC 7396 patch and arrays are replaced
 * wholesale, not merged. Any change to the base window block has to be made in
 * all three.
 */
import { platform } from "@tauri-apps/plugin-os";

export type OSName = "macos" | "windows" | "linux";

/**
 * Everything Tauri doesn't build for — BSDs and Solaris are all in `platform()`'s
 * union — is treated as Linux, since that's the desktop stack they run.
 */
function detect(): OSName {
  try {
    const p = platform();
    if (p === "macos") return "macos";
    if (p === "windows") return "windows";
    return "linux";
  } catch {
    // The plugin's constant is injected by the Tauri runtime, so it isn't there
    // when the dev server is opened in a plain browser. Guessing from the user
    // agent keeps that case working rather than crashing before first paint;
    // it is never the path a packaged build takes.
    const ua = navigator.userAgent;
    if (ua.includes("Mac")) return "macos";
    if (ua.includes("Windows")) return "windows";
    return "linux";
  }
}

export const OS = detect();
export const IS_MAC = OS === "macos";

/**
 * Publishes the platform to CSS. Called before the first render, so the rules
 * keyed on `[data-os]` — the corner radius, the shape of the window controls —
 * are in force for the first paint rather than applied a frame later.
 */
export function applyPlatform() {
  document.documentElement.dataset.os = OS;
}
