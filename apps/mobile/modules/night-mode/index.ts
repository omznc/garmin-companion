import { requireOptionalNativeModule } from "expo";

type NativeNightMode = {
  /** Keeps the light or dark choice for the next start of the app. */
  save(scheme: "system" | "light" | "dark"): void;
};

/** The native module, or null where it does not exist: iOS, the web, and an
 *  Android build without the module (Expo Go). iOS needs no copy: the
 *  JavaScript sets the style of the window before the first screen draws. */
export const NightMode = requireOptionalNativeModule<NativeNightMode>("NightMode");
