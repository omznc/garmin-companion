import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "expo-router";
import { Alert } from "react-native";
import { setThemePref } from "../theme/prefs";
import { setTheme, signOut } from "./account";
import type { ThemeMode } from "./types";
import { useSettings } from "./useAccount";
import { useSync } from "./useSync";

export const THEMES: { value: ThemeMode; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

// Everything the Settings screens do. The screens only draw.
export function useSettingsActions() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const settings = useSettings();
  const sync = useSync();

  const runSync = (args: { days?: number; full?: boolean }) => sync.run(args).catch(() => {});

  const chooseTheme = async (theme: ThemeMode) => {
    setThemePref("scheme", theme);
    await setTheme(theme).catch(() => {});
    queryClient.invalidateQueries({ queryKey: ["settings"] });
  };

  const disconnect = () =>
    Alert.alert(
      "Disconnect Garmin?",
      "The app removes the sign-in from this phone. Your saved data stays.",
      [
        { text: "Cancel", style: "cancel" },
        {
          text: "Disconnect",
          style: "destructive",
          onPress: async () => {
            queryClient.setQueryData(["status"], await signOut());
            queryClient.invalidateQueries({ queryKey: ["settings"] });
            router.dismissAll();
            router.replace("/setup");
          },
        },
      ],
    );

  return { settings, sync, runSync, chooseTheme, disconnect };
}
