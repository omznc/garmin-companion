// Material 3 colour for Android only. Import this file from `.android.tsx` files.
import { isDynamicColorAvailable, useMaterialColors } from "@expo/ui/jetpack-compose";

// The ember accent of the desktop app. It seeds the scheme below Android 12.
export const SEED_COLOR = "#b0563a";

// Material You on Android 12 and later. A scheme from the seed colour below that.
export const HOST_SEED = isDynamicColorAvailable ? undefined : SEED_COLOR;

export function useMd() {
  return useMaterialColors(isDynamicColorAvailable ? undefined : { seedColor: SEED_COLOR });
}
