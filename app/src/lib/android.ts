/**
 * The window's own bindings, on Android.
 *
 * `MainActivity.onWebViewCreate` binds one object to the webview before the
 * first `loadUrl`, so everything here is answerable on the app's first line
 * rather than an IPC round trip away — the same guarantee `platform.ts`
 * documents for `platform()` and `COMPOSITES_ALPHA`, and for the same reason:
 * these decide what the window looks like, and an asynchronous answer means a
 * frame of the wrong thing on every launch.
 *
 * Three things pass through it, and they are all the same question from
 * different sides — where the app's surface ends and the system's begins.
 * `lib/dynamic.ts` turns the colours into a palette; `lib/theme.ts` drives the
 * bar appearance from whichever palette wins; the inset below is for the one
 * page in this app that isn't this app.
 *
 * Every export is a no-op off Android. Callers don't branch on the platform.
 */

interface Bridge {
  dynamicColors(): string;
  setBarAppearance(light: boolean): void;
  setEdgeToEdge(on: boolean, backdrop: string): void;
}

interface Installer {
  canInstall(): boolean;
  openPermissionSettings(): void;
  install(path: string): string;
}

interface Sharer {
  share(path: string): string;
}

function bridge(): Bridge | null {
  const w = window as { __GARMIN_ANDROID__?: Bridge };
  return w.__GARMIN_ANDROID__ ?? null;
}

function installer(): Installer | null {
  const w = window as { __GARMIN_INSTALL__?: Installer };
  return w.__GARMIN_INSTALL__ ?? null;
}

function sharer(): Sharer | null {
  const w = window as { __GARMIN_SHARE__?: Sharer };
  return w.__GARMIN_SHARE__ ?? null;
}

/**
 * Android's five tonal ramps as raw JSON, or null where there aren't any —
 * a desktop, or a phone older than Android 12.
 */
export function readDynamicColors(): string | null {
  const raw = bridge()?.dynamicColors();
  // Empty is the honest answer from a phone with no dynamic colour to give,
  // rather than an error worth reporting.
  return raw ? raw : null;
}

/**
 * Which way round to draw the status and navigation bar icons.
 *
 * `enableEdgeToEdge()` sets this from the system's light/dark setting, which is
 * the wrong question: the app has an appearance of its own and the two disagree
 * whenever someone picks light on a dark phone, or wears a palette with a fixed
 * appearance. The result was white icons on a white page.
 */
export function setBarAppearance(light: boolean): void {
  bridge()?.setBarAppearance(light);
}

/**
 * Whether the page is allowed to draw under the status and navigation bars.
 *
 * On by default and true for every screen this app draws — the shell pads
 * itself out of the way with `env(safe-area-inset-*)` and the scroll fade
 * softens what passes behind the status bar, which is the whole look.
 *
 * Turned off for exactly one thing: the Garmin sign-in page. On a phone that
 * loads in this same webview (there is only one — see `login.rs`), and it is
 * Garmin's HTML, which has never heard of this window and lays its header out
 * at y=0. Edge-to-edge puts that header under the notification bar and behind
 * the camera cutout. Nothing in this app can reach into their page to fix it,
 * so the window stops being edge-to-edge for as long as their page is in it.
 *
 * `backdrop` fills the band the insets free up, and decides which way round the
 * bar icons go. It is the caller's because only the caller knows what colour
 * the page it is about to load is.
 *
 * Turning it back on is not this side's responsibility, and shouldn't be: the
 * only page that could ask is the one that loads *after* sign-in, and whether
 * it ever gets to depends on a navigation this app doesn't drive. Missing it
 * strands the window inset until a force-quit. The activity watches for the
 * webview coming home and restores it itself — see `watchForReturn`. The call
 * in `main.tsx` is the fast path, not the mechanism.
 */
export function setEdgeToEdge(on: boolean, backdrop = "#000000"): void {
  bridge()?.setEdgeToEdge(on, backdrop);
}

/**
 * Whether the phone would currently let this app install another one.
 *
 * False is the normal first answer, not a fault. "Install unknown apps" is
 * granted per-app from Android 8, and the app that holds it is whichever
 * browser the APK was originally downloaded through — which is never this one.
 * `askToInstall` is the way out, and it is a settings screen rather than a
 * dialog, so nothing here can wait for the answer.
 *
 * True off Android, where the question doesn't arise and the caller shouldn't
 * have to know that.
 */
export function canInstallApk(): boolean {
  return installer()?.canInstall() ?? true;
}

/** Open the system page that grants it, scoped to this app. */
export function askToInstall(): void {
  installer()?.openPermissionSettings();
}

/**
 * Hand a downloaded APK to the system installer.
 *
 * Returns "" when the request went in, or a sentence when it didn't — and note
 * that "" means the system's confirmation is about to appear, not that anything
 * has been installed. Whether it is depends on a button in a dialog this app
 * doesn't own; a refusal comes back later through `onInstallFailed`.
 */
export function installApk(path: string): string {
  const bound = installer();
  if (!bound) return "this build can't install updates";
  return bound.install(path);
}

/**
 * Be told when an install the user was asked about didn't happen.
 *
 * The failure is minutes after the call and arrives from Kotlin rather than
 * from a promise, because the intervening time is spent in the system
 * installer. Success never arrives — this process is replaced by the version
 * that would have received it.
 */
/**
 * Open the system sharesheet on a rendered card.
 *
 * The path is one Rust wrote into this app's private cache, which no other app
 * can read by path — the bridge wraps it in a `FileProvider` URI and grants the
 * chosen app a one-shot read. Same mechanism as `installApk`, and for the same
 * reason: handing another process a raw path stopped working in Android 7.
 *
 * Returns "" when the sheet went up, or a sentence when it didn't. Which app
 * the user picks, or whether they pick one at all, never comes back — Android
 * doesn't say, and the button doesn't claim to know.
 */
export function shareFile(path: string): string {
  const bound = sharer();
  if (!bound) return "this build can't share";
  return bound.share(path);
}

export function onInstallFailed(fn: (message: string) => void): void {
  (window as { __GARMIN_INSTALL_FAILED__?: (m: string) => void }).__GARMIN_INSTALL_FAILED__ = fn;
}
