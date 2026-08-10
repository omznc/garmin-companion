/**
 * What happens when the coach's notification is tapped.
 *
 * The promise a notification makes is that there is more behind it, and the
 * whole reason the brief is one piece of writing — an `alert` for the lock
 * screen and a `body` for the app — is so that tapping lands on the rest of the
 * same thought rather than merely opening the app somewhere.
 *
 * Two halves, because a tap can arrive in two very different situations:
 *
 * 1. **The app is running.** The plugin emits `actionPerformed`, this hears it,
 *    and the screen is told to open the block. A plain tap counts: Android's
 *    side of the plugin sends one under the id `tap`, so no custom action type
 *    has to be registered for the ordinary case.
 *
 * 2. **The app was not running.** The tap *is* the launch, and by the time this
 *    module has been imported the event is long gone — there was nothing
 *    listening when it fired and the plugin does not replay it. Nothing here
 *    can catch that one, so Today asks the question from the other end instead
 *    (`notified && !read`): something knocked today, and the block hasn't been
 *    opened yet, so open it. See `Today`.
 *
 * `addPluginListener` comes from the API package rather than
 * `@tauri-apps/plugin-notification`, which the app doesn't otherwise need —
 * every notification is built and scheduled in Rust, and this one subscription
 * is the entire JavaScript surface of that plugin.
 */
import { addPluginListener, type PluginListener } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { BRIEF_ID } from "./api";

/**
 * The key the brief's id travels under. Set by `deliver` on the Rust side, and
 * the reason a tap on some future notification that isn't a brief won't be read
 * as a request to open one.
 */
const TAP_KEY = "brief";

/** The shape of the payload, as much of it as this cares about. */
interface TappedNotification {
  extra?: Record<string, unknown> | null;
}

/* --------------------------------------------------------------- the store --- */

/**
 * Whether the brief's block has been asked to open itself.
 *
 * A module-level flag rather than router state: a tap is an event, not a place,
 * and putting it in the URL would mean a reload or a back button could
 * re-trigger it. `useSyncExternalStore` is what lets a component subscribe to
 * it without any of that.
 */
let asked = false;
const listeners = new Set<() => void>();

function publish() {
  for (const fire of listeners) fire();
}

/** Ask the Today screen to open the brief. */
export function focusBrief() {
  if (asked) return;
  asked = true;
  publish();
}

/**
 * Put the flag down once the block has done whatever opening means to it.
 * Without this the block would re-scroll on every unrelated re-render.
 */
export function clearBriefFocus() {
  if (!asked) return;
  asked = false;
  publish();
}

export function useBriefFocus(): boolean {
  return useSyncExternalStore(
    (fire) => {
      listeners.add(fire);
      return () => listeners.delete(fire);
    },
    () => asked,
  );
}

/* ------------------------------------------------------------ the listener --- */

/**
 * Subscribe to notification taps for as long as the app is running.
 *
 * Resolves to an unsubscribe. On a platform where the plugin has no such event
 * — every desktop, where a notification is a banner that can be clicked but not
 * reported on — the subscription simply never fires, which is why the failure
 * here is swallowed rather than surfaced.
 */
export async function onBriefTap(open: () => void): Promise<() => void> {
  let listener: PluginListener | undefined;
  try {
    listener = await addPluginListener<TappedNotification>(
      "notification",
      "actionPerformed",
      (payload) => {
        if (payload?.extra?.[TAP_KEY] === BRIEF_ID) open();
      },
    );
  } catch {
    // No such plugin event on this platform. The cold-start path in `Today`
    // still covers the case that matters.
  }
  return () => listener?.unregister();
}
