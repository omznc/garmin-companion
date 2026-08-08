/**
 * The Android half of updating, which is the half Tauri doesn't do.
 *
 * `tauri-plugin-updater` reads a manifest, downloads a bundle and swaps it on
 * disk. The first two of those are just as true here; the third isn't, because
 * an Android package is replaced by the system rather than by the app. So the
 * plugin isn't compiled in on this target (see `src-tauri/Cargo.toml`) and what
 * is left is written out here — a manifest, a download, and a handover.
 *
 * The old note that Android "can't self-update" was wrong. What it can't do is
 * update *silently*: `ApkInstaller` on the Kotlin side commits a package
 * installer session, the system draws its own confirmation over the app, and
 * the user taps once. Everything before that tap happens without them.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/**
 * Where the Android build looks for a new version.
 *
 * The second hardcoded reference to the repo slug — `plugins.updater.endpoints`
 * in `tauri.conf.json` is the other, and it is the desktop equivalent of this
 * line. They are separate files because they describe separate artifacts, but
 * they move together: if the repo is ever renamed, both change or half the
 * installs stop hearing about releases. RELEASING says so in one place.
 *
 * `releases/latest/download/…` rather than a pinned URL, because GitHub
 * redirects it to whichever release is currently published — which is also why
 * a draft release is invisible to this until someone hits publish.
 */
const MANIFEST =
  "https://github.com/omznc/garmin-companion/releases/latest/download/latest-android.json";

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
 * Null rather than throwing for the ordinary absences — no release yet, no
 * manifest attached to it, a phone with no signal. None of those are faults and
 * all of them would otherwise read to the user as "update failed".
 */
export async function latestApk(): Promise<ApkRelease | null> {
  let body: unknown;
  try {
    // `no-store` because the whole point of asking is to find out whether the
    // answer changed, and this is asked once per launch at most.
    const resp = await fetch(MANIFEST, { cache: "no-store" });
    if (!resp.ok) return null;
    body = await resp.json();
  } catch {
    return null;
  }

  const m = body as Partial<ApkRelease> | null;
  if (!m?.version || !m.url || !m.sha256) return null;
  return { version: m.version, url: m.url, sha256: m.sha256.toLowerCase() };
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
