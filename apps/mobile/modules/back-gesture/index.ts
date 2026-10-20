import { requireOptionalNativeModule } from "expo";

type NativeBackGesture = {
  /** True: Android handles Back (the back-to-home preview). False: the app. */
  setSystemOwnsBack(owns: boolean): void;
};

/** The native module, or null where it does not exist: iOS, the web, and an
 *  Android build without the module (Expo Go). */
export const BackGesture = requireOptionalNativeModule<NativeBackGesture>("BackGesture");
