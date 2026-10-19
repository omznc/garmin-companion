// Reads app.json and lets the release build set the version.
// The version in app.json is the floor for a local build. It must not be lower than the
// latest release, or the updater offers the release to a local build.
// Raise it after each release.
// APP_VERSION is "major.minor.patch". The version code follows the formula of the
// old Tauri app (major * 1000000 + minor * 1000 + patch), so it always grows.
const { AndroidConfig, withAndroidColorsNight } = require("expo/config-plugins");

// The window colour behind every screen, before the app draws and during a screen
// transition. It is the surface of the Material 3 baseline scheme. At run time
// useRootColor.android.ts sets the exact colour of the theme.
const BACKGROUND_LIGHT = "#fef7ff";
const BACKGROUND_DARK = "#141218";

// expo-system-ui writes the light colour. This writes the dark colour to values-night.
const withDarkWindowBackground = (config) =>
  withAndroidColorsNight(config, (night) => {
    night.modResults = AndroidConfig.Colors.assignColorValue(night.modResults, {
      name: "activityBackground",
      value: BACKGROUND_DARK,
    });
    return night;
  });

const withVersion = ({ config }) => {
  const version = process.env.APP_VERSION || config.version;
  const [major, minor, patch] = version.split(".").map(Number);
  if ([major, minor, patch].some((n) => !Number.isInteger(n))) {
    throw new Error(`The version must be major.minor.patch, got "${version}"`);
  }
  return {
    ...config,
    version,
    android: {
      ...config.android,
      backgroundColor: BACKGROUND_LIGHT,
      versionCode: major * 1000000 + minor * 1000 + patch,
    },
  };
};

module.exports = (ctx) => withDarkWindowBackground(withVersion(ctx));
