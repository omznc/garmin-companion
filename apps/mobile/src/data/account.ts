// Typed calls for sign-in, sync and Settings. The core owns the sentences.
import { File, Paths } from "expo-file-system";
import { Appearance } from "react-native";
import { callJson, leaveSampleData } from "./source";
import type { SettingsView, Status, SyncSummary, ThemeMode } from "./types";

export const getStatus = () => callJson<Status>("status");
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
const skipMarker = () => new File(Paths.document, "sample-mode");
export const skipSetup = () => {
  if (!skipMarker().exists) skipMarker().create();
};
export const clearSkipSetup = () => {
  if (skipMarker().exists) skipMarker().delete();
};
export const setupSkipped = () => process.env.EXPO_PUBLIC_FIXTURE === "1" || skipMarker().exists;
