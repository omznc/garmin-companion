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
 *
 * Android reaches the same place by a different road, and the shape of the
 * difference is worth knowing before reading the branch below. A desktop build
 * downloads *and installs* in the background and is left needing a restart. An
 * Android build can only download in the background: replacing the package is
 * the system's to do and it asks first, so the last step is a tap rather than a
 * relaunch. Both end at "there is a newer version here, say when" — which is
 * why they share one state machine and one `apply()`, and why `ready` is worth
 * reading as "nothing left but your decision" rather than as "installed".
 */
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { getVersion } from "@tauri-apps/api/app";
import { askToInstall, canInstallApk, installApk, onInstallFailed } from "./android";
import { fetchApk, isNewer, latestApk } from "./apk";
import { IS_MOBILE } from "./platform";

export type UpdateState =
  | { at: "idle" }
  | { at: "checking" }
  | { at: "current" }
  | { at: "downloading"; version: string; pct: number | null }
  | { at: "ready"; version: string; notes?: string }
  /**
   * Android only: downloaded, and the phone won't let this app install it until
   * "install unknown apps" is granted. Its own state rather than a `failed`,
   * because nothing has gone wrong and there is a specific thing to offer.
   */
  | { at: "blocked"; version: string }
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

/** Where the staged APK is, on Android, once there is one. */
let staged: string | null = null;

/**
 * Check, and install anything found. Safe to call repeatedly — a second call
 * while one is running joins the first rather than starting a parallel
 * download.
 */
export function runUpdate(): Promise<void> {
  if (inFlight) return inFlight;
  // Nothing to gain from checking again once a version is staged; the decision
  // is the only thing left to make.
  if (state.at === "ready" || state.at === "blocked") return Promise.resolve();

  inFlight = (IS_MOBILE ? runAndroid() : runDesktop()).finally(() => {
    inFlight = null;
  });
  return inFlight;
}

async function runDesktop(): Promise<void> {
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
  }
}

/**
 * The same three steps, minus the install.
 *
 * The version comparison is done here rather than by a server: `latest.json`
 * on desktop is read by a plugin that knows what version it is running, and the
 * Android manifest is a flat file with no such reader. Doing it in two lines is
 * cheaper than either signing a second manifest format or asking GitHub's API
 * per launch.
 */
async function runAndroid(): Promise<void> {
  set({ at: "checking" });
  try {
    const [release, current] = await Promise.all([latestApk(), getVersion()]);
    if (!release || !isNewer(release.version, current)) {
      set({ at: "current" });
      return;
    }

    set({ at: "downloading", version: release.version, pct: null });
    const apk = await fetchApk(release, (pct) =>
      set({ at: "downloading", version: release.version, pct }),
    );
    staged = apk.path;
    set({ at: "ready", version: release.version });

    // Downloading is the part worth doing behind someone's back; interrupting
    // them with a system dialog is not, so a fresh download waits in Settings.
    // A *stale* one is different: it was fetched in some earlier session, the
    // app has just started, and a launch is the cheapest moment there is to be
    // asked to replace it. Once per version, so declining is respected rather
    // than re-asked on every open.
    //
    // Not without the permission, though. `apply` would send them to a settings
    // screen to grant it, and doing that seconds after launch — to someone who
    // hasn't asked for anything — is the app walking out of itself. Granting it
    // is a decision that belongs next to the explanation in Settings.
    const KEY = "apk-offered";
    if (!apk.fresh && canInstallApk() && localStorage.getItem(KEY) !== release.version) {
      localStorage.setItem(KEY, release.version);
      apply();
    }
  } catch (e) {
    set({ at: "failed", message: describe(e) });
  }
}

/**
 * Take the update that's waiting — restart into it on desktop, hand it to the
 * system installer on Android.
 *
 * Both are "the last step", and neither returns in any meaningful sense: one
 * relaunches the process and the other has the system replace it. The only way
 * back here is a failure.
 */
export function apply(): void {
  if (!IS_MOBILE) {
    void relaunch();
    return;
  }
  if (state.at !== "ready" && state.at !== "blocked") return;
  const version = state.version;
  if (!staged) {
    set({ at: "failed", message: "the download is no longer there" });
    return;
  }

  // Asked at the moment of use rather than at download time: it is a settings
  // screen away, and prompting for it while fetching something the user hasn't
  // yet said they want is the wrong order.
  if (!canInstallApk()) {
    set({ at: "blocked", version });
    askToInstall();
    return;
  }

  onInstallFailed((message) => set({ at: "failed", message }));
  const problem = installApk(staged);
  if (problem) set({ at: "failed", message: problem });
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
