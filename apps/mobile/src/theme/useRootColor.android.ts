// The window behind the app follows the theme, so no light flash shows under the
// navigation bar or between screens in dark mode.
import { useEffect } from "react";
import * as SystemUI from "expo-system-ui";
import { useMd } from "./md";

export function useRootColor(): string {
  const { surface } = useMd();
  useEffect(() => {
    SystemUI.setBackgroundColorAsync(surface).catch(() => {});
  }, [surface]);
  return surface;
}
