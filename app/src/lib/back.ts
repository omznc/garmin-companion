/**
 * Android's back gesture, answered by the app.
 *
 * Tauri's `TauriActivity` sets `handleBackNavigation = false`, so nothing was
 * registered for it and every back — from a lap list, from Settings, from four
 * screens deep — closed the app. That is the single loudest way this didn't
 * behave like an Android app: back is the system's one universal control and it
 * did the most destructive thing available, everywhere.
 *
 * The activity now asks here first, and only falls through to the webview's own
 * history when nothing on this side wants the press. That split is deliberate:
 *
 * - What's *open* is something only the app knows about. A bottom sheet is not
 *   a history entry, and back has to close it rather than navigate out from
 *   underneath it. That's what the handlers below are for.
 * - Where you've *been* is something the webview already tracks exactly, hash
 *   navigations included. Re-deriving it here would mean shadowing the router's
 *   history with a counter that goes wrong the first time something calls
 *   `replace` — so the Kotlin asks `canGoBack()` and nothing here has an
 *   opinion about it.
 *
 * See `MainActivity.onWebViewCreate` for the other half.
 */
import { IS_MOBILE } from "./platform";

/** Returns whether it consumed the press. */
type Handler = () => boolean;

/**
 * Innermost last. Walked from the top, so a sheet opened over a dialog gets the
 * press before the dialog does — which is the order they're stacked on screen.
 */
const handlers: Handler[] = [];

/**
 * Take the back press while something is open. Returns the way to stop.
 *
 * Register on open and drop on close rather than registering once and checking
 * a flag: a handler that is only there while it can act means the fall-through
 * below needs no bookkeeping to know whether anything handled the press.
 */
export function onBack(handler: Handler): () => void {
  handlers.push(handler);
  return () => {
    const i = handlers.indexOf(handler);
    if (i >= 0) handlers.splice(i, 1);
  };
}

interface BackWindow {
  __GARMIN_BACK__?: () => boolean;
}

/**
 * Publish the entry point the activity calls.
 *
 * Android only. Every other platform has a window with a title bar and its own
 * idea of what closing means, and none of them route a hardware button through
 * the page.
 */
export function startBack(): void {
  if (!IS_MOBILE) return;
  (window as BackWindow).__GARMIN_BACK__ = () => {
    for (let i = handlers.length - 1; i >= 0; i--) {
      if (handlers[i]()) return true;
    }
    return false;
  };
}
