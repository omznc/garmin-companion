// Material 3 colour for Android only. Import this file from `.android.tsx` files.
// The scheme comes from the app's own choice (`prefs.ts`), not from the phone alone,
// so a theme switch inside the app reaches every native control at once.
import { isDynamicColorAvailable, useMaterialColors } from "@expo/ui/jetpack-compose";
import { useColorScheme } from "react-native";
import { useThemePrefs } from "./prefs";
import { THEMES } from "./themes";

export const isMaterialYouAvailable = isDynamicColorAvailable;

/** The scheme and the seed for a `Host`. Spread them on every Host. */
export function useHostTheme() {
  const prefs = useThemePrefs();
  const system = useColorScheme();
  const colorScheme: "light" | "dark" =
    prefs.scheme === "system" ? (system === "dark" ? "dark" : "light") : prefs.scheme;
  const dynamic = prefs.materialYou && isDynamicColorAvailable;
  const seed = (THEMES.find((t) => t.id === prefs.theme) ?? THEMES[0]).seed;
  return { colorScheme, seedColor: dynamic ? undefined : seed };
}

export function useMd() {
  const { colorScheme, seedColor } = useHostTheme();
  return useMaterialColors({ colorScheme, seedColor });
}
