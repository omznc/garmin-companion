const path = require("node:path");
const { getDefaultConfig } = require("expo/metro-config");

const config = getDefaultConfig(__dirname);

// The fixture of the core crate is outside the app folder.
config.watchFolders = [path.resolve(__dirname, "../../crates/garmin-api/fixtures")];

module.exports = config;
