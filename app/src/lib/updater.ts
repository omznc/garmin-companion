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
 * Three roads reach the same place, and the shape of the difference is worth
 * knowing before reading the branches below.
 *
 * - **macOS, Windows, and a Linux AppImage** download *and install* in the
 *   background, and are left needing a restart.
 * - **A Linux `.deb` or `.rpm`** downloads in the background and stops there.
 *   Installing means replacing a system package, which needs root, and a
 *   password dialog nobody asked for is not a thing to raise at launch — so
 *   that half waits behind the button. See `installNeedsRoot`.
 * - **Android** can only ever download in the background: replacing the
 *   package is the system's to do and it asks first, so the last step is a tap
 *   rather than a relaunch.
 *
 * All three end at "there is a newer version here, say when" — which is why
 * they share one state machine and one `apply()`, and why `ready` is worth
 * reading as "nothing left but your decision" rather than as "installed".
 */
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { BundleType, getBundleType, getVersion } from "@tauri-apps/api/app";
import { askToInstall, canInstallApk, installApk, onInstallFailed } from "./android";
import { fetchApk, isNewer, latestApk, releaseNotes } from "./apk";
import { IS_MOBILE } from "./platform";

export type UpdateState =
  | { at: "idle" }
  | { at: "checking" }
  | { at: "current" }
  | { at: "downloading"; version: string; pct: number | null }
  /**
   * `needsRoot` is the Linux system-package case and the reason `ready` is not
   * simply "installed" — see [`installNeedsRoot`]. Everywhere else the install
   * has already happened by the time this state is reached and the flag is
   * false.
   */
  | { at: "ready"; version: string; notes?: string; needsRoot: boolean }
  /** Only reachable when `needsRoot`: the seconds the password dialog is up. */
  | { at: "installing"; version: string }
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
 * The downloaded-but-not-installed update, on Linux system packages only.
 *
 * The plugin holds the bytes against this object's resource id, so `install()`
 * has to be called on the same one `download()` was — which is the whole
 * reason it's kept rather than re-checked.
 */
let pending: Update | null = null;

/**
 * Whether installing an update on this build needs root, which on Linux
 * depends entirely on how it was installed.
 *
 * An AppImage is a file the user owns: the plugin swaps it in place and
 * nothing asks anything. A `.deb` or `.rpm` is a system package, and the
 * plugin replaces it by running `dpkg -i` / `rpm -U` under `pkexec` — so a
 * password dialog appears, drawn by polkit rather than by us, and there is no
 * version of "update a system-wide package" that doesn't.
 *
 * What we can decide is *when*. `downloadAndInstall` does both halves in one
 * call, which put that dialog on screen four seconds after launch, unasked,
 * while the check ran in the background. Splitting it moves the prompt behind
 * the button in Settings, where the person pressing it has been told what it's
 * for.
 *
 * Read once and remembered: it's fixed for the life of the binary, baked in by
 * the bundler.
 */
let needsRoot: Promise<boolean> | null = null;
function installNeedsRoot(): Promise<boolean> {
  needsRoot ??= getBundleType()
    .then((type) => type === BundleType.Deb || type === BundleType.Rpm)
    // An older Tauri, or a build no bundler ever stamped — `cargo tauri dev`
    // among them. False is the safe answer: it costs a prompt nobody was
    // warned about, where true would offer an install button on a build that
    // has already installed itself.
    .catch(() => false);
  return needsRoot;
}

/**
 * Check, and install anything found. Safe to call repeatedly — a second call
 * while one is running joins the first rather than starting a parallel
 * download.
 */
export function runUpdate(): Promise<void> {
  if (inFlight) return inFlight;
  // Nothing to gain from checking again once a version is staged; the decision
  // is the only thing left to make — or, while a password dialog is up, has
  // been made and is being waited on.
  if (state.at === "ready" || state.at === "blocked" || state.at === "installing") {
    return Promise.resolve();
  }

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
    const onProgress = (e: DownloadEvent) => {
      if (e.event === "Started") total = e.data.contentLength ?? 0;
      else if (e.event === "Progress") {
        got += e.data.chunkLength;
        set({
          at: "downloading",
          version: update.version,
          pct: total ? got / total : null,
        });
      }
    };

    // The only split in this function, and it's about *when* a password is
    // asked for rather than whether — see `installNeedsRoot`. Downloading is
    // the part worth doing in the background; putting a polkit dialog on
    // screen unprompted is not.
    const root = await installNeedsRoot();
    if (root) {
      await update.download(onProgress);
      pending = update;
    } else {
      await update.downloadAndInstall(onProgress);
    }

    set({ at: "ready", version: update.version, notes: update.body, needsRoot: root });
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
    // Started alongside the download rather than after it. It's a small request
    // to a different host than the APK comes from, nothing about installing
    // waits on it, and it resolves to undefined rather than throwing — so the
    // worst it can do to the line below is leave the notes out.
    const notes = releaseNotes(release.version);
    const apk = await fetchApk(release, (pct) =>
      set({ at: "downloading", version: release.version, pct }),
    );
    staged = apk.path;
    // Never on Android: the system installer asks for a tap, not a password.
    set({ at: "ready", version: release.version, notes: await notes, needsRoot: false });

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
    // A Linux system package is the one desktop case where the new version
    // isn't on disk yet, because installing it needs root and asking for that
    // in the background is not on. So this button does the install, and the
    // password dialog belongs to the press rather than to the launch.
    if (state.at === "ready" && state.needsRoot && pending) {
      const update = pending;
      const staged = state;
      set({ at: "installing", version: staged.version });
      void update
        .install()
        // Same ending as every other platform: installed, and the running
        // process is the only stale thing left.
        .then(() => relaunch())
        // Dismissing the password dialog is a decision, and it costs nothing:
        // the download is still there. So the offer goes back up rather than
        // turning into a failure to start over from.
        .catch((e) => set(dismissed(e) ? staged : { at: "failed", message: describe(e) }));
      return;
    }
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

/**
 * Whether the polkit dialog was dismissed rather than anything going wrong.
 * Matched on the plugin's own wording, which is the only signal it gives.
 */
function dismissed(e: unknown): boolean {
  return /authentication failed or was cancelled/i.test(e instanceof Error ? e.message : String(e));
}

/** Being offline is normal and shouldn't read like a fault. Nor is declining. */
function describe(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  if (/network|dns|connect|timed? out|resolve/i.test(raw)) {
    return "couldn't reach the update server";
  }
  // Reachable only when the split in `runDesktop` didn't happen — a build
  // whose bundle type couldn't be read, on a system package. The prompt then
  // arrives during the background download, which is the case this whole
  // arrangement exists to avoid, so say plainly what happened.
  if (dismissed(e)) return "not installed — the password prompt was dismissed";
  return `update failed — ${raw}`;
}
