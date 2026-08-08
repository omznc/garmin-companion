import { useCallback, useSyncExternalStore } from "react";
import { getThemeState, setPalette, setTheme, subscribe, type Mode } from "./theme";

export function useTheme() {
  const { theme, palette, mode, preset, custom, customs } = useSyncExternalStore(
    subscribe,
    getThemeState,
  );

  /**
   * The sidebar's toggle: light and dark only.
   *
   * "Match system" belongs in Settings — it's a preference you set once, not
   * something you want to land on halfway through flipping the lights. From
   * "system" this picks whichever of the two you aren't currently looking at,
   * so the click always visibly does something.
   *
   * It reads `mode` rather than `theme`, so under a palette it would flip away
   * from what's on screen — but the sidebar doesn't offer it there. A palette
   * is a fixed appearance, and a control that silently threw one away to change
   * the lighting would be the wrong trade to make on someone's behalf.
   */
  const toggle = useCallback(() => {
    setTheme(mode === "dark" ? "light" : "dark");
  }, [mode]);

  /** The theme you'd get by clicking, not the one you're on. */
  const next: Mode = mode === "dark" ? "light" : "dark";
  const label = next === "light" ? "Light mode" : "Dark mode";

  return {
    theme,
    setTheme,
    palette,
    setPalette,
    /** Whichever kind of palette is in force. Both null means the built-in. */
    preset,
    custom,
    /** Everything in the themes folder. */
    customs,
    /** What to call the palette in force, if there is one. */
    paletteName: preset?.name ?? custom?.name ?? null,
    mode,
    toggle,
    next,
    label,
  };
}
