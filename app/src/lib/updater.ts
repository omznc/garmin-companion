/**
 * Auto-update, run once per app launch.
 *
 * This lives outside React because the work is not owned by any screen: the
 * check starts at boot whatever you happen to be looking at, and has to
 * survive you navigating away from Settings mid-download. Components subscribe
 * to the result; they don't drive it.
 *
 * The sequence is deliberate — find, download, install, then stop. The new
 * version only takes effect on restart, so there is never a moment where the
 * app closes itself out from under you. Restarting is offered, never taken.
 */
import { check } from "@tauri-apps/plugin-updater";
import { IS_MOBILE } from "./platform";

export type UpdateState =
  | { at: "idle" }
  | { at: "checking" }
  | { at: "current" }
  | { at: "downloading"; version: string; pct: number | null }
  | { at: "ready"; version: string; notes?: string }
  | { at: "failed"; message: string };

let state: UpdateState = { at: "idle" };
const listeners = new Set<() => void>();

function set(next: UpdateState) {
  state = next;
  listeners.forEach((l) => l());
}

export function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** `useSyncExternalStore` compares by identity, so this must not rebuild. */
export function getUpdateState(): UpdateState {
  return state;
}

let inFlight: Promise<void> | null = null;

/**
 * Check, and install anything found. Safe to call repeatedly — a second call
 * while one is running joins the first rather than starting a parallel
 * download.
 */
export function runUpdate(): Promise<void> {
  if (inFlight) return inFlight;
  // Nothing to gain from checking again once a version is staged; the restart
  // is the only thing left to do.
  if (state.at === "ready") return Promise.resolve();
  // The updater swaps out the whole app bundle, which an installed Android app
  // may not do — so the plugin isn't compiled in on mobile (see `Cargo.toml`)
  // and `check()` would reject with an unrecognised-command error. Staying
  // `idle` is the truthful state: nothing was checked, and nothing failed.
  if (IS_MOBILE) return Promise.resolve();

  inFlight = (async () => {
    set({ at: "checking" });
    try {
      const update = await check();
      if (!update) {
        set({ at: "current" });
        return;
      }

      set({ at: "downloading", version: update.version, pct: null });

      // A total arrives only when the server sent a content-length, so the
      // progress bar has to cope with never knowing the size.
      let total = 0;
      let got = 0;
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        else if (e.event === "Progress") {
          got += e.data.chunkLength;
          set({
            at: "downloading",
            version: update.version,
            pct: total ? got / total : null,
          });
        }
      });

      set({ at: "ready", version: update.version, notes: update.body });
    } catch (e) {
      set({ at: "failed", message: describe(e) });
    } finally {
      inFlight = null;
    }
  })();

  return inFlight;
}

/**
 * Kick the check off shortly after launch rather than immediately: startup is
 * already busy opening the cache and drawing the first screen, and an update
 * is never urgent enough to compete with that.
 */
export function startAutoUpdate(delayMs = 4000): void {
  setTimeout(() => void runUpdate(), delayMs);
}

/** Being offline is normal and shouldn't read like a fault. */
function describe(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  return /network|dns|connect|timed? out|resolve/i.test(raw)
    ? "couldn't reach the update server"
    : `update failed — ${raw}`;
}
