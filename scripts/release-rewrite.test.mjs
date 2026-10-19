import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { buildManifest, collect } from "./release-rewrite.mjs";

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

test("manifest ignores the unsigned iOS IPA", () => {
  const dir = dirWith([...FILES, `garmin-companion_${V}_unsigned.ipa`]);
  const urls = Object.values(buildManifest(V, "o/r", "n", dir).platforms).map((p) => p.url);
  assert.ok(urls.every((u) => !u.endsWith(".ipa")));
});

test("manifest refuses a missing platform", () => {
  assert.throws(() => buildManifest(V, "o/r", "n", dirWith(FILES.slice(0, 3))), /missing platform/);
});

test("collect copies the dmg of a macOS target without a signature", () => {
  const bundle = mkdtempSync(join(tmpdir(), "rb-"));
  const out = join(mkdtempSync(join(tmpdir(), "ro-")), "out");
  mkdirSync(join(bundle, "macos"));
  mkdirSync(join(bundle, "dmg"));
  writeFileSync(join(bundle, "macos", "Garmin Companion.app.tar.gz"), "x");
  writeFileSync(join(bundle, "macos", "Garmin Companion.app.tar.gz.sig"), "sig");
  writeFileSync(join(bundle, "dmg", "Garmin Companion_1.0.0_aarch64.dmg"), "x");
  collect(bundle, "aarch64-apple-darwin", V, out);
  assert.deepEqual(readdirSync(out).sort(), [
    `garmin-companion_${V}_aarch64.app.tar.gz`,
    `garmin-companion_${V}_aarch64.app.tar.gz.sig`,
    `garmin-companion_${V}_aarch64.dmg`,
  ]);
  // The manifest ignores the dmg.
  assert.throws(() => buildManifest(V, "o/r", "n", out), /missing platform/);
});

test("collect fails when the dmg is missing", () => {
  const bundle = mkdtempSync(join(tmpdir(), "rb-"));
  writeFileSync(join(bundle, "a.app.tar.gz"), "x");
  writeFileSync(join(bundle, "a.app.tar.gz.sig"), "sig");
  assert.throws(() => collect(bundle, "aarch64-apple-darwin", V, join(bundle, "out")), /\.dmg/);
});

test("collect still fails when the updater bundle is missing", () => {
  const bundle = mkdtempSync(join(tmpdir(), "rb-"));
  assert.throws(() => collect(bundle, "aarch64-apple-darwin", V, join(bundle, "out")), /expected one/);
});
