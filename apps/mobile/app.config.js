// Reads app.json and lets the release build set the version.
// The version in app.json is the floor for a local build. It must not be lower than the
// latest release, or the updater offers the release to a local build.
// Raise it after each release.
// APP_VERSION is "major.minor.patch". The version code follows the formula of the
// old Tauri app (major * 1000000 + minor * 1000 + patch), so it always grows.
module.exports = ({ config }) => {
  const version = process.env.APP_VERSION || config.version;
  const [major, minor, patch] = version.split(".").map(Number);
  if ([major, minor, patch].some((n) => !Number.isInteger(n))) {
    throw new Error(`The version must be major.minor.patch, got "${version}"`);
  }
  return {
    ...config,
    version,
    android: { ...config.android, versionCode: major * 1000000 + minor * 1000 + patch },
  };
};
