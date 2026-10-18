// Typed calls for sign-in, sync and Settings. The core owns the sentences.
import { Appearance } from "react-native";
import { callJson } from "./source";
import type { SettingsView, Status, SyncSummary, ThemeMode } from "./types";

export const getStatus = () => callJson<Status>("status");
export const getSignInUrl = () => callJson<string>("sign_in_url");
export const redeemTicket = (ticket: string) => callJson<Status>("redeem_ticket", { ticket });
export const signOut = () => callJson<Status>("sign_out");
export const getSettings = () => callJson<SettingsView>("settings");
export const setTheme = (theme: ThemeMode) =>
  callJson<{ theme: ThemeMode }>("set_theme", { theme });
export const runSync = (args: { days?: number; full?: boolean }) =>
  callJson<SyncSummary>("sync", args);

// "system" (unspecified) clears the override, so the phone setting rules again.
export const applyTheme = (theme: ThemeMode) =>
  Appearance.setColorScheme(theme === "system" ? "unspecified" : theme);

// Sample data only: skip Setup for this run.
let skippedSetup = false;
export const skipSetup = () => {
  skippedSetup = true;
};
export const setupSkipped = () => skippedSetup;
