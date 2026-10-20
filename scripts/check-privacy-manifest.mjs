// Check that the iOS app declares the required-reason APIs that it uses.
// The App Store refuses a build that uses such an API without a reason.
// Usage: node scripts/check-privacy-manifest.mjs [path/to/PrivacyInfo.xcprivacy]
// With no argument, the script reads ios.privacyManifests of apps/mobile/app.json.
// With a path, the script reads the manifest that prebuild or Xcode made.
import { readFileSync } from "node:fs";

const REQUIRED = [
  "FileTimestamp",
  "UserDefaults",
  "SystemBootTime",
  "DiskSpace",
].map((name) => `NSPrivacyAccessedAPICategory${name}`);

const file = process.argv[2];
let declared;
if (file) {
  // A plist is XML. The category names are plain text in the file.
  const text = readFileSync(file, "utf8");
  declared = REQUIRED.filter((category) => text.includes(category));
} else {
  const app = JSON.parse(
    readFileSync(new URL("../apps/mobile/app.json", import.meta.url), "utf8"),
  );
  const types = app.expo?.ios?.privacyManifests?.NSPrivacyAccessedAPITypes ?? [];
  declared = types
    .filter((type) => type.NSPrivacyAccessedAPITypeReasons?.length > 0)
    .map((type) => type.NSPrivacyAccessedAPIType);
}

const missing = REQUIRED.filter((category) => !declared.includes(category));
if (missing.length > 0) {
  console.error(`::error::Missing privacy manifest entries: ${missing.join(", ")}`);
  process.exit(1);
}
console.log(`Privacy manifest declares ${REQUIRED.length} required-reason categories.`);
