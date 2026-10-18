// Config plugin for the standalone-APK self-update.
// EXPO_PUBLIC_SELF_UPDATE=0 (Play build) leaves REQUEST_INSTALL_PACKAGES out of the manifest.
// Any other value, or no value, adds it. The app code reads the same variable.
// GC_TEST_CLEARTEXT=1 allows http. Use it only for a test build with a local server.
const { withAndroidManifest } = require("expo/config-plugins");

const PERMISSION = "android.permission.REQUEST_INSTALL_PACKAGES";

module.exports = function withSelfUpdate(config) {
  return withAndroidManifest(config, (cfg) => {
    const manifest = cfg.modResults.manifest;
    const perms = (manifest["uses-permission"] ??= []);
    const has = perms.some((p) => p.$["android:name"] === PERMISSION);
    if (process.env.EXPO_PUBLIC_SELF_UPDATE !== "0" && !has) {
      perms.push({ $: { "android:name": PERMISSION } });
    }
    if (process.env.GC_TEST_CLEARTEXT === "1") {
      manifest.application[0].$["android:usesCleartextTraffic"] = "true";
    }
    return cfg;
  });
};
