// JS side of the Android self-update module. Other platforms have no native module.
import { NativeModule, requireOptionalNativeModule } from "expo";

type Events = {
  progress: (e: { received: number; total: number }) => void;
  installFailed: (e: { message: string }) => void;
};

declare class ApkUpdaterModule extends NativeModule<Events> {
  installedVersion(): string;
  isNewer(version: string): boolean;
  canInstall(): boolean;
  openPermissionSettings(): void;
  download(url: string, version: string, sha256: string): Promise<string>;
  install(path: string): Promise<void>;
}

export const ApkUpdater = requireOptionalNativeModule<ApkUpdaterModule>("ApkUpdater");
