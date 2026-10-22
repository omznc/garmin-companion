import { requireOptionalNativeModule } from "expo";

type NativeSyncService = {
  /** Starts the foreground service. False if the system refuses it, for example in the background. */
  start(): boolean;
  stop(): void;
};

/** The native module, or null where it does not exist: iOS, the web, and an Android build
 *  without the module (Expo Go). The system lets a foreground app use the network. Android 15
 *  blocks the network of a background app, and a foreground service ends that block. */
export const SyncService = requireOptionalNativeModule<NativeSyncService>("SyncService");
