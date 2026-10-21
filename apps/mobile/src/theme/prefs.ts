// How the app looks on this phone: light or dark, the theme colour and Material You.
// The choices live in a small file on the phone. The file is read before the first
// screen draws, so the first frame has the right colours.
import { File, Paths } from "expo-file-system";
import { useSyncExternalStore } from "react";
import { Appearance } from "react-native";
import { NightMode } from "../../modules/night-mode";
import { type ThemeId, THEMES } from "./themes";

export type SchemePref = "system" | "light" | "dark";
export type ThemePrefs = {
  scheme: SchemePref;
  theme: ThemeId;
  materialYou: boolean;
  highContrast: boolean;
  noVibration: boolean;
};

const DEFAULTS: ThemePrefs = {
  scheme: "system",
  theme: "ember",
  materialYou: true,
  highContrast: false,
  noVibration: false,
};
const file = () => new File(Paths.document, "theme.json");

const read = (): ThemePrefs => {
  try {
    const p = JSON.parse(file().textSync()) as Partial<ThemePrefs>;
    return {
      scheme: ["system", "light", "dark"].includes(p.scheme as string)
        ? (p.scheme as SchemePref)
        : DEFAULTS.scheme,
      theme: THEMES.some((t) => t.id === p.theme) ? (p.theme as ThemeId) : DEFAULTS.theme,
      materialYou: typeof p.materialYou === "boolean" ? p.materialYou : DEFAULTS.materialYou,
      highContrast: typeof p.highContrast === "boolean" ? p.highContrast : DEFAULTS.highContrast,
      noVibration: typeof p.noVibration === "boolean" ? p.noVibration : DEFAULTS.noVibration,
    };
  } catch {
    // No file yet, or an unreadable one: the defaults are a complete theme.
    return DEFAULTS;
  }
};

// "system" clears the override, so the phone setting rules again.
// On Android a copy goes to the native side. The next cold start reads it before
// the first frame (modules/night-mode).
const applyScheme = (scheme: SchemePref) => {
  NightMode?.save(scheme);
  Appearance.setColorScheme(scheme === "system" ? "unspecified" : scheme);
};

let current = read();
applyScheme(current.scheme);

const listeners = new Set<() => void>();

export const setThemePref = <K extends keyof ThemePrefs>(key: K, value: ThemePrefs[K]) => {
  current = { ...current, [key]: value };
  if (key === "scheme") applyScheme(current.scheme);
  for (const l of listeners) l();
  try {
    const f = file();
    if (!f.exists) f.create();
    f.write(JSON.stringify(current));
  } catch {
    // The choice still applies until the app closes. It is only not kept.
  }
};

/** The current choice, for code that is not a component (the haptics). */
export const getThemePrefs = (): ThemePrefs => current;

export const useThemePrefs = (): ThemePrefs =>
  useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => current,
  );
