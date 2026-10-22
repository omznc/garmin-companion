// The notification choices and the memory of what went out. Two small files in the document
// folder. The background task reads them each time, because the app can change them in
// another JavaScript context.
import { File, Paths } from "expo-file-system";
import { useSyncExternalStore } from "react";
import { DEFAULT_PREFS, EMPTY_STATE, type NotifyPrefs, type NotifyState } from "./plan";

const prefsFile = () => new File(Paths.document, "notifications.json");
const stateFile = () => new File(Paths.document, "notified.json");

function readJson<T>(file: File, fallback: T): T {
  try {
    return { ...fallback, ...JSON.parse(file.textSync()) };
  } catch {
    // No file yet, or an unreadable one: start from the default.
    return fallback;
  }
}

function writeJson(file: File, value: unknown) {
  try {
    if (!file.exists) file.create();
    file.writeSync(JSON.stringify(value));
  } catch {
    // A lost write only means a choice that does not stay. It must not break a sync.
  }
}

export const readPrefs = (): NotifyPrefs => readJson(prefsFile(), DEFAULT_PREFS);
export const readState = (): NotifyState => readJson(stateFile(), EMPTY_STATE);
export const writeState = (state: NotifyState) => writeJson(stateFile(), state);

// A sign-in or a sign-out starts the memory again. The next account must not get news of the old one.
export const resetNotifyState = () => writeState(EMPTY_STATE);

const listeners = new Set<() => void>();
let cached: NotifyPrefs | null = null;

export function setNotifyPrefs(patch: Partial<NotifyPrefs>) {
  cached = { ...readPrefs(), ...patch };
  writeJson(prefsFile(), cached);
  listeners.forEach((l) => l());
}

export function useNotifyPrefs(): NotifyPrefs {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => {
      cached ??= readPrefs();
      return cached;
    },
  );
}
