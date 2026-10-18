// The supporting text of the Appearance row in More: the theme in force.
import { isMaterialYouAvailable } from "./md";
import { useThemePrefs } from "./prefs";
import { THEMES } from "./themes";

export function useAppearanceLine(_scheme: string | undefined): string {
  const p = useThemePrefs();
  if (p.materialYou && isMaterialYouAvailable) return "Material You";
  return THEMES.find((t) => t.id === p.theme)?.label ?? "Ember";
}
