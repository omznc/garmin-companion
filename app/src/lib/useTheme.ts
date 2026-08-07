import { useCallback, useEffect, useState } from "react";
import { applyTheme, loadTheme, watchSystemTheme, type Theme } from "./theme";

const ORDER: Theme[] = ["light", "dark", "system"];

/** The label names the theme you'd get by clicking, not the one you're on. */
const NEXT_LABEL: Record<Theme, string> = {
  light: "Dark mode",
  dark: "Match system",
  system: "Light mode",
};

export function useTheme() {
  const [theme, setTheme] = useState<Theme>(loadTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  // Only "system" needs to react to the OS flipping.
  useEffect(() => {
    if (theme !== "system") return;
    return watchSystemTheme(() => applyTheme("system"));
  }, [theme]);

  const cycle = useCallback(() => {
    setTheme((t) => ORDER[(ORDER.indexOf(t) + 1) % ORDER.length]);
  }, []);

  return { theme, setTheme, cycle, label: NEXT_LABEL[theme] };
}
