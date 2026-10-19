import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { buildManifest } from "./release-rewrite.mjs";

const V = "1.0.0";
const FILES = [
  `garmin-companion_${V}_x64-setup.exe`,
  `garmin-companion_${V}_aarch64.app.tar.gz`,
  `garmin-companion_${V}_x64.app.tar.gz`,
  `garmin-companion_${V}_amd64.AppImage`,
  `garmin-companion_${V}_amd64.deb`,
  `garmin-companion-${V}-1.x86_64.rpm`,
];

function dirWith(files) {
  const dir = mkdtempSync(join(tmpdir(), "rr-"));
  for (const f of files) {
    writeFileSync(join(dir, f), "x");
    writeFileSync(join(dir, `${f}.sig`), `sig-${f}\n`);
  }
  return dir;
}

test("manifest has every platform key and the full signature", () => {
  const m = buildManifest(V, "o/r", "notes", dirWith(FILES));
  assert.equal(m.version, V);
  assert.equal(m.platforms["windows-x86_64"].signature, `sig-garmin-companion_${V}_x64-setup.exe`);
  assert.equal(
    m.platforms["darwin-aarch64"].url,
    `https://github.com/o/r/releases/latest/download/garmin-companion_${V}_aarch64.app.tar.gz`,
  );
  assert.ok(m.platforms["linux-x86_64-deb"] && m.platforms["linux-x86_64-rpm"]);
  assert.ok(!Number.isNaN(Date.parse(m.pub_date)));
});

test("manifest refuses a missing platform", () => {
  assert.throws(() => buildManifest(V, "o/r", "n", dirWith(FILES.slice(0, 3))), /missing platform/);
});
