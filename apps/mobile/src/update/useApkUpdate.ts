// State of the APK self-update. Android only, and only in a self-update build.
// The Play build sets EXPO_PUBLIC_SELF_UPDATE=0. Then this hook stays idle.
import { useEffect, useSyncExternalStore } from "react";
import { Platform } from "react-native";
import { ApkUpdater } from "../../modules/apk-updater";
import { type ApkRelease, fetchManifest } from "./manifest";

export const SELF_UPDATE =
  Platform.OS === "android" && process.env.EXPO_PUBLIC_SELF_UPDATE !== "0" && !!ApkUpdater;

export type UpdateState =
  | { at: "idle" }
  | { at: "available"; release: ApkRelease }
  | { at: "downloading"; release: ApkRelease; received: number; total: number }
  | { at: "ready"; release: ApkRelease; path: string }
  // The user must allow "Install unknown apps" first. Android has no callback for it.
  | { at: "blocked"; release: ApkRelease; path: string }
  | { at: "failed"; release: ApkRelease; message: string };

// The result of the last look at the manifest. The Settings row shows it.
export type CheckState = "none" | "checking" | "current" | "offline";

// One store for the banner and the Settings row, so both show the same update.
type Snapshot = { state: UpdateState; check: CheckState };
let snapshot: Snapshot = { state: { at: "idle" }, check: "none" };
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => listeners.delete(listener);
};
const getSnapshot = () => snapshot;
function patch(next: Partial<Snapshot>) {
  snapshot = { ...snapshot, ...next };
  for (const listener of listeners) listener();
}
const setState = (next: UpdateState | ((s: UpdateState) => UpdateState)) =>
  patch({ state: typeof next === "function" ? next(snapshot.state) : next });

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

async function check() {
  if (!SELF_UPDATE || !ApkUpdater || snapshot.check === "checking") return;
  patch({ check: "checking" });
  try {
    const release = await fetchManifest();
    if (release && ApkUpdater.isNewer(release.version)) {
      patch({ check: "current", state: { at: "available", release } });
    } else {
      patch({ check: "current" });
    }
  } catch {
    patch({ check: "offline" }); // Offline is not an error worth a banner.
  }
}

async function install(release: ApkRelease, path: string) {
  if (!ApkUpdater) return;
  if (!ApkUpdater.canInstall()) {
    ApkUpdater.openPermissionSettings();
    setState({ at: "blocked", release, path });
    return;
  }
  try {
    setState({ at: "ready", release, path });
    await ApkUpdater.install(path);
  } catch (e) {
    setState({ at: "failed", release, message: message(e) });
  }
}

async function start(release: ApkRelease) {
  if (!ApkUpdater) return;
  setState({ at: "downloading", release, received: 0, total: 0 });
  try {
    const path = await ApkUpdater.download(release.url, release.version, release.sha256);
    await install(release, path);
  } catch (e) {
    setState({ at: "failed", release, message: message(e) });
  }
}

const dismiss = () => setState({ at: "idle" });

// Check once per launch.
let launched = false;
function checkOnLaunch() {
  if (launched) return;
  launched = true;
  void check();
}

export function useApkUpdate() {
  const { state, check: checkState } = useSyncExternalStore(subscribe, getSnapshot);

  useEffect(() => {
    if (!SELF_UPDATE || !ApkUpdater) return;
    checkOnLaunch();
    const progress = ApkUpdater.addListener("progress", ({ received, total }) =>
      setState((s) => (s.at === "downloading" ? { ...s, received, total } : s)),
    );
    const failed = ApkUpdater.addListener("installFailed", ({ message: m }) =>
      setState((s) => ("release" in s ? { at: "failed", release: s.release, message: m } : s)),
    );
    return () => {
      progress.remove();
      failed.remove();
    };
  }, []);

  return { state, checkState, check, start, install, dismiss };
}
