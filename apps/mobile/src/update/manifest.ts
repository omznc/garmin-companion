// The file `latest-android.json` that the release job writes next to the APK:
// { "version": "0.5.0", "url": "https://...apk", "sha256": "<64 hex>" }
export type ApkRelease = { version: string; url: string; sha256: string };

export const MANIFEST_URL =
  process.env.EXPO_PUBLIC_UPDATE_MANIFEST_URL ??
  "https://github.com/omznc/garmin-companion/releases/latest/download/latest-android.json";

/** Returns the release, or null when the manifest is not a complete one. */
export function parseManifest(json: unknown): ApkRelease | null {
  if (typeof json !== "object" || json === null) return null;
  const { version, url, sha256 } = json as Record<string, unknown>;
  if (typeof version !== "string" || typeof url !== "string" || typeof sha256 !== "string") {
    return null;
  }
  const hash = sha256.toLowerCase();
  if (!version || !url || !/^[0-9a-f]{64}$/.test(hash)) return null;
  return { version, url, sha256: hash };
}

/** Returns null for "no update info" (a 404 or a bad file). Throws when the network fails. */
export async function fetchManifest(): Promise<ApkRelease | null> {
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), 15_000);
  try {
    const resp = await fetch(MANIFEST_URL, { signal: ctl.signal });
    if (!resp.ok) return null;
    return parseManifest(await resp.json().catch(() => null));
  } finally {
    clearTimeout(timer);
  }
}
