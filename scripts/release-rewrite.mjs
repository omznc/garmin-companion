#!/usr/bin/env node
// Helpers for .github/workflows/release-rewrite.yml.
//
//   collect <bundle-dir> <target-triple> <version> <out-dir>
//       Copy the updater bundles and their .sig files to plain file names.
//       Copy the .dmg of a macOS target too. It has no .sig file.
//       A missing .dmg is a warning, not an error.
//   manifest <version> <repo> <notes-file> <in-dir> <out-file>
//       Write the Tauri v2 latest.json from the .sig files in <in-dir>.
//
// The old Tauri updater reads the manifest. It checks that every platform
// entry is complete, so this script refuses to write a partial manifest.
import { copyFileSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

// File name suffix of a bundle -> [updater platform keys it serves].
// The updater tries "{os}-{arch}-{installer}" first, then "{os}-{arch}".
const BUNDLES = {
  "x86_64-pc-windows-msvc": [
    { find: /-setup\.exe$/, name: (v) => `garmin-companion_${v}_x64-setup.exe`, keys: ["windows-x86_64", "windows-x86_64-nsis"] },
  ],
  "aarch64-apple-darwin": [
    { find: /\.app\.tar\.gz$/, name: (v) => `garmin-companion_${v}_aarch64.app.tar.gz`, keys: ["darwin-aarch64", "darwin-aarch64-app"] },
    // For a new user. The updater does not read it, so it has no .sig and no key.
    { find: /\.dmg$/, name: (v) => `garmin-companion_${v}_aarch64.dmg`, keys: [], sig: false, optional: true },
  ],
  "x86_64-apple-darwin": [
    { find: /\.app\.tar\.gz$/, name: (v) => `garmin-companion_${v}_x64.app.tar.gz`, keys: ["darwin-x86_64", "darwin-x86_64-app"] },
    { find: /\.dmg$/, name: (v) => `garmin-companion_${v}_x64.dmg`, keys: [], sig: false, optional: true },
  ],
  "x86_64-unknown-linux-gnu": [
    { find: /\.AppImage$/, name: (v) => `garmin-companion_${v}_amd64.AppImage`, keys: ["linux-x86_64", "linux-x86_64-appimage"] },
    { find: /\.deb$/, name: (v) => `garmin-companion_${v}_amd64.deb`, keys: ["linux-x86_64-deb"] },
    { find: /\.rpm$/, name: (v) => `garmin-companion-${v}-1.x86_64.rpm`, keys: ["linux-x86_64-rpm"] },
  ],
};

// Keys the old app can ask for. Every one of them must be present.
const REQUIRED = [
  "windows-x86_64",
  "darwin-aarch64",
  "darwin-x86_64",
  "linux-x86_64",
  "linux-x86_64-appimage",
  "linux-x86_64-deb",
  "linux-x86_64-rpm",
];

function walk(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) =>
    e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)],
  );
}

export function collect(bundleDir, target, version, outDir) {
  const rules = BUNDLES[target];
  if (!rules) throw new Error(`unknown target ${target}`);
  mkdirSync(outDir, { recursive: true });
  const files = walk(bundleDir);
  for (const rule of rules) {
    const hits = files.filter((f) => rule.find.test(f));
    if (rule.optional && hits.length === 0) {
      console.warn(`warning: no ${rule.find} in ${bundleDir}, skipped`);
      continue;
    }
    if (hits.length !== 1) throw new Error(`expected one ${rule.find} in ${bundleDir}, found ${hits.length}`);
    const signed = rule.sig !== false;
    if (signed && !files.includes(`${hits[0]}.sig`)) throw new Error(`no .sig file for ${hits[0]}`);
    const out = join(outDir, rule.name(version));
    copyFileSync(hits[0], out);
    if (signed) copyFileSync(`${hits[0]}.sig`, `${out}.sig`);
    console.log(`${hits[0]} -> ${out}`);
  }
}

export function buildManifest(version, repo, notes, inDir) {
  const names = new Set(readdirSync(inDir));
  const platforms = {};
  for (const rules of Object.values(BUNDLES)) {
    for (const rule of rules) {
      const name = rule.name(version);
      if (rule.keys.length === 0 || !names.has(name) || !names.has(`${name}.sig`)) continue;
      const entry = {
        // The whole .sig file, as the file holds it. The updater decodes it itself.
        signature: readFileSync(join(inDir, `${name}.sig`), "utf8").trim(),
        url: `https://github.com/${repo}/releases/latest/download/${name}`,
      };
      for (const key of rule.keys) platforms[key] = entry;
    }
  }
  const missing = REQUIRED.filter((k) => !platforms[k]);
  if (missing.length) throw new Error(`missing platform entries: ${missing.join(", ")}`);
  return { version, notes, pub_date: new Date().toISOString(), platforms };
}

const [cmd, ...args] = process.argv.slice(2);
if (cmd === "collect") {
  collect(...args);
} else if (cmd === "manifest") {
  const [version, repo, notesFile, inDir, outFile] = args;
  const notes = readFileSync(notesFile, "utf8").trim();
  writeFileSync(outFile, `${JSON.stringify(buildManifest(version, repo, notes, inDir), null, 2)}\n`);
} else if (cmd) {
  console.error("usage: release-rewrite.mjs collect|manifest ...");
  process.exit(2);
}
