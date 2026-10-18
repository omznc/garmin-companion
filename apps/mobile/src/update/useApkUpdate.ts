// State of the APK self-update. Android only, and only in a self-update build.
// The Play build sets EXPO_PUBLIC_SELF_UPDATE=0. Then this hook stays idle.
import { useCallback, useEffect, useState } from "react";
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

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function useApkUpdate() {
  const [state, setState] = useState<UpdateState>({ at: "idle" });

  // Check once per launch.
  useEffect(() => {
    if (!SELF_UPDATE || !ApkUpdater) return;
    const native = ApkUpdater;
    let live = true;
    fetchManifest()
      .then((release) => {
        if (live && release && native.isNewer(release.version)) {
          setState({ at: "available", release });
        }
      })
      .catch(() => {}); // Offline is not an error worth a banner.
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    if (!SELF_UPDATE || !ApkUpdater) return;
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

  const install = useCallback(async (release: ApkRelease, path: string) => {
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
  }, []);

  const start = useCallback(
    async (release: ApkRelease) => {
      if (!ApkUpdater) return;
      setState({ at: "downloading", release, received: 0, total: 0 });
      try {
        const path = await ApkUpdater.download(release.url, release.version, release.sha256);
        await install(release, path);
      } catch (e) {
        setState({ at: "failed", release, message: message(e) });
      }
    },
    [install],
  );

  const dismiss = useCallback(() => setState({ at: "idle" }), []);

  return { state, start, install, dismiss };
}
