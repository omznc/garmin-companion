// Typed calls for sign-in, sync and Settings. The core owns the sentences.
import { File, Paths } from "expo-file-system";
import { Appearance } from "react-native";
import { callJson, leaveSampleData } from "./source";
import type { SettingsView, Status, SyncSummary, ThemeMode } from "./types";

// The core ignores the sample flag when it has tokens, for example after a stop before the first sync.
export const getStatus = async () => {
  const status = await callJson<Status>("status");
  if (status.connected) leaveSampleData();
  return status;
};
export const getSignInUrl = () => callJson<string>("sign_in_url");
export const redeemTicket = async (ticket: string) => {
  const status = await callJson<Status>("redeem_ticket", { ticket });
  clearSkipSetup();
  leaveSampleData();
  return status;
};
export const signOut = () => callJson<Status>("sign_out");
export const getSettings = () => callJson<SettingsView>("settings");
export const setTheme = (theme: ThemeMode) =>
  callJson<{ theme: ThemeMode }>("set_theme", { theme });
export const runSync = (args: { days?: number; full?: boolean }) =>
  callJson<SyncSummary>("sync", args);

// "system" (unspecified) clears the override, so the phone setting rules again.
export const applyTheme = (theme: ThemeMode) =>
  Appearance.setColorScheme(theme === "system" ? "unspecified" : theme);

// Sample data only: the choice to skip Setup lives in a marker file, so it lasts across launches.
// Sign-in removes it. A build with EXPO_PUBLIC_FIXTURE=1 always skips Setup.
// Expo can write only in the document folder (`files/` on Android). The core removes it there too.
const skipMarker = () => new File(Paths.document, "sample-mode");
export const skipSetup = () => {
  if (!skipMarker().exists) skipMarker().create();
};
export const clearSkipSetup = () => {
  if (skipMarker().exists) skipMarker().delete();
};
export const setupSkipped = () => process.env.EXPO_PUBLIC_FIXTURE === "1" || skipMarker().exists;

// The full history import is long. The marker lets the app resume it after the app stops.
const historyMarker = () => new File(Paths.document, "history-import");
export const markHistoryPending = () => {
  if (!historyMarker().exists) historyMarker().create();
};
export const clearHistoryPending = () => {
  if (historyMarker().exists) historyMarker().delete();
};
export const historyPending = () => historyMarker().exists;

// The time of the last good sync, in milliseconds. The app uses it to start a quick sync when it
// opens. The core has the same fact, but it gives only a sentence for it.
const syncedMarker = () => new File(Paths.document, "last-sync");
export const markSynced = () => {
  try {
    const marker = syncedMarker();
    if (!marker.exists) marker.create();
    marker.write(String(Date.now()));
  } catch {
    // A missing marker only means one more quick sync at the next start.
  }
};
export const lastSyncedAt = (): number | null => {
  try {
    const marker = syncedMarker();
    const at = marker.exists ? Number(marker.textSync()) : Number.NaN;
    return Number.isFinite(at) ? at : null;
  } catch {
    return null;
  }
};
