// Config plugin: keep the app files off every Android copy path.
// The app keeps secrets.bin, secrets.key and the cache in its private directory.
// allowBackup="false" stops Auto Backup. On Android 12 and later, device-to-device
// transfer needs dataExtractionRules too. The old Tauri app had both.
const { withAndroidManifest, withDangerousMod } = require("expo/config-plugins");
const { mkdirSync, writeFileSync } = require("node:fs");
const { join } = require("node:path");

const DOMAINS = ["root", "file", "database", "sharedpref", "external"];
const block = (tag) =>
  `    <${tag}>\n${DOMAINS.map((d) => `        <exclude domain="${d}" />`).join("\n")}\n    </${tag}>\n`;
const RULES = `<?xml version="1.0" encoding="utf-8"?>\n<data-extraction-rules>\n${block("cloud-backup")}${block("device-transfer")}</data-extraction-rules>\n`;

module.exports = function withDataExtraction(config) {
  config = withAndroidManifest(config, (cfg) => {
    cfg.modResults.manifest.application[0].$["android:dataExtractionRules"] =
      "@xml/data_extraction_rules";
    return cfg;
  });
  return withDangerousMod(config, [
    "android",
    (cfg) => {
      const dir = join(cfg.modRequest.platformProjectRoot, "app/src/main/res/xml");
      mkdirSync(dir, { recursive: true });
      writeFileSync(join(dir, "data_extraction_rules.xml"), RULES);
      return cfg;
    },
  ]);
};
