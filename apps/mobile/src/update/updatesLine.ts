// The value and the action of the Updates row in More > Settings.
import type { useApkUpdate } from "./useApkUpdate";

type Api = ReturnType<typeof useApkUpdate>;

export function updatesLine(api: Api): { value: string; busy: boolean; onPress?: () => void } {
  const { state: s } = api;
  switch (s.at) {
    case "available":
      return {
        value: `Version ${s.release.version} is available. Tap to install.`,
        busy: false,
        onPress: () => api.start(s.release),
      };
    case "downloading":
      return { value: `Downloading version ${s.release.version}`, busy: true };
    case "ready":
      return { value: `Installing version ${s.release.version}`, busy: true };
    case "blocked":
      return {
        value: 'Turn on "Install unknown apps", then tap here',
        busy: false,
        onPress: () => api.install(s.release, s.path),
      };
    case "failed":
      return {
        value: `Update failed: ${s.message}. Tap to retry.`,
        busy: false,
        onPress: () => api.start(s.release),
      };
    case "idle":
      break;
  }
  const busy = api.checkState === "checking";
  const value = {
    none: "Tap to check",
    checking: "Checking",
    current: "Up to date",
    offline: "Could not check. Tap to retry.",
  }[api.checkState];
  return { value, busy, onPress: () => api.check() };
}
