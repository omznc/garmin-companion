/**
 * The Android half of updating, which is the half Tauri doesn't do.
 *
 * `tauri-plugin-updater` reads a manifest, downloads a bundle and swaps it on
 * disk. The first two of those are just as true here; the third isn't, because
 * an Android package is replaced by the system rather than by the app. So the
 * plugin isn't compiled in on this target (see `src-tauri/Cargo.toml`) and what
 * is left is written out here — a manifest, a download, and a handover.
 *
 * Two of those three are Rust commands rather than code in this file, and for
 * the same reason: GitHub serves release assets from a host that sends no CORS
 * header, so a webview may not read them however plainly the URL is written.
 * What stays here is the sequencing and the one request GitHub does allow — its
 * REST API, which sets `Access-Control-Allow-Origin`.
 *
 * The old note that Android "can't self-update" was wrong. What it can't do is
 * update *silently*: `ApkInstaller` on the Kotlin side commits a package
 * installer session, the system draws its own confirmation over the app, and
 * the user taps once. Everything before that tap happens without them.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * Where the notes come from, which is not the manifest.
 *
 * The manifest is written by the build, and the notes are written by a person
 * afterwards — the workflow opens the draft with an empty body and it's filled
 * in before publishing. So there is no moment during the build at which the
 * text exists to be embedded, and the only copy is the release itself.
 *
 * Third reference to the repo slug — `ANDROID_MANIFEST` in `lib.rs` and
 * `plugins.updater.endpoints` in `tauri.conf.json` are the others — and the one
 * that isn't interchangeable with them: `api.github.com` has no
 * `releases/latest/download` equivalent, so this is a real API call. It happens
 * once per new version rather than once per launch — the desktop plugin gets
 * `body` for free with the manifest it already fetches, and this is Android
 * paying for the same thing separately.
 *
 * It is also the one GitHub host this page may talk to directly: the API sends
 * `Access-Control-Allow-Origin: *` where the asset host sends nothing at all,
 * which is why the notes can be fetched here and the manifest cannot.
 */
const RELEASE_API = "https://api.github.com/repos/omznc/garmin-companion/releases/latest";

/** What `.github/workflows/release.yml` writes beside the APK. */
export interface ApkRelease {
  version: string;
  url: string;
  /** Lowercase hex. Checked in Rust against what actually arrived. */
  sha256: string;
}

/** Where a fetched APK ended up. */
export interface StagedApk {
  path: string;
  /**
   * False when it was already on disk from an earlier launch — which is the
   * signal `lib/updater.ts` uses to decide it may interrupt with the install
   * prompt, having cost the user nothing this time round.
   */
  fresh: boolean;
}

/**
 * The published Android release, or null if there isn't one to be had.
 *
 * The reading is done in Rust — see `latest_apk` in `lib.rs`, which is also
 * where the URL lives and where the reason is written down. In short: the
 * manifest sits behind a GitHub redirect to a host that sends no CORS header,
 * so this page is not permitted to fetch it and never was. Every check said
 * "up to date" until v0.4.0 made that visibly wrong.
 *
 * Null is an absence — no release, or no manifest on it yet. A failure to ask
 * at all throws, and `runAndroid` reports it as one, because a check that
 * couldn't happen is not the same answer as a check that came back empty.
 */
export async function latestApk(): Promise<ApkRelease | null> {
  return await invoke<ApkRelease | null>("latest_apk");
}

/**
 * The Markdown body of the published release, for showing beside "downloaded".
 *
 * Undefined for every failure and every absence, because notes are decoration:
 * an update with none is still an update, and nothing here should be able to
 * turn a successful download into a failed one.
 *
 * The version is passed in and checked rather than trusted, since this and the
 * manifest are two requests to two hosts and a release can be published between
 * them. Showing the notes for a version other than the one that was just
 * downloaded is worse than showing none.
 */
export async function releaseNotes(version: string): Promise<string | undefined> {
  try {
    const resp = await fetch(RELEASE_API, { cache: "no-store" });
    if (!resp.ok) return undefined;
    const release = (await resp.json()) as { tag_name?: string; body?: string } | null;
    // Tags carry a leading `v` and the manifest's version doesn't — see
    // RELEASING, where the two are bumped together.
    if (release?.tag_name?.replace(/^v/, "") !== version) return undefined;
    return release.body?.trim() || undefined;
  } catch {
    return undefined;
  }
}

/**
 * Fetch it, reporting progress as a 0–1 fraction — or null where the server
 * didn't say how big it was, which the bar has to be able to draw.
 */
export async function fetchApk(
  release: ApkRelease,
  onProgress: (pct: number | null) => void,
): Promise<StagedApk> {
  const stop = await listen<{ received: number; total: number }>("apk-download", (e) => {
    onProgress(e.payload.total ? e.payload.received / e.payload.total : null);
  });
  try {
    return await invoke<StagedApk>("download_apk", {
      url: release.url,
      version: release.version,
      sha256: release.sha256,
    });
  } finally {
    stop();
  }
}

/**
 * Whether `a` is a later version than `b`.
 *
 * Numeric, part by part, because the strings sort wrong the moment a component
 * reaches ten — "0.10.0" is below "0.9.0" alphabetically. Only the three
 * numbers are compared: this project's versions have never had a suffix, and
 * guessing at pre-release ordering for one that doesn't exist would be a rule
 * nobody could check.
 */
export function isNewer(a: string, b: string): boolean {
  const parts = (v: string) => v.split(".").map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    const d = (x[i] ?? 0) - (y[i] ?? 0);
    if (d !== 0) return d > 0;
  }
  return false;
}
