// Material 3 colour for Android only. Import this file from `.android.tsx` files.
// The scheme comes from the app's own choice (`prefs.ts`), not from the phone alone,
// so a theme switch inside the app reaches every native control at once.
import { isDynamicColorAvailable, useMaterialColors } from "@expo/ui/jetpack-compose";
import { useMemo } from "react";
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

const channels = (hex: string) => [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16));

/** Mix two "#RRGGBBAA" colours. A weight of 1 gives the first. The alpha stays opaque. */
const mix = (a: string, b: string, weight: number): `#${string}` => {
  const from = channels(a);
  const to = channels(b);
  const out = from.map((c, i) => Math.round(c * weight + (to[i] ?? 0) * (1 - weight)));
  return `#${out.map((c) => c.toString(16).padStart(2, "0")).join("")}ff`;
};

type Md = ReturnType<typeof useMaterialColors>;

/**
 * High contrast: the quiet text moves towards the main text, and the lines
 * move one step darker. The surfaces stay, because Compose draws its controls
 * from the same scheme.
 */
export function withHighContrast(md: Md): Md {
  return {
    ...md,
    onSurfaceVariant: mix(md.onSurfaceVariant, md.onSurface, 0.6),
    outline: md.onSurfaceVariant,
    outlineVariant: md.outline,
  };
}

export function useMd() {
  const { colorScheme, seedColor } = useHostTheme();
  const { highContrast } = useThemePrefs();
  const md = useMaterialColors({ colorScheme, seedColor });
  return useMemo(() => (highContrast ? withHighContrast(md) : md), [md, highContrast]);
}
