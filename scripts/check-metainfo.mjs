#!/usr/bin/env node
/**
 * Checks that the AppStream metadata agrees with the version being shipped.
 *
 * `app/src-tauri/linux/com.omznc.garmincompanion.metainfo.xml` carries a
 * `<releases>` list, and GNOME Software and KDE Discover read the newest entry
 * in it as both "what's new" and "what version this is". Nothing in the build
 * derives it, so it is hand-written, and hand-written means forgettable — and
 * forgetting it fails nowhere: the packages build, install and run, they just
 * advertise the previous version to every software centre on Linux.
 *
 * That is the failure this exists to make loud. It runs in CI on every push,
 * so the mismatch shows up on the commit that caused it rather than on a
 * release page a week later.
 *
 * The prose in each entry stays hand-written on purpose. Software centres show
 * it to someone deciding whether to install, and a list of commit subjects —
 * which is the right answer for a release page, and is what
 * `scripts/changelog.mjs` generates — reads as noise there.
 */
import { readFileSync } from "node:fs";

const CONF = "app/src-tauri/tauri.conf.json";
const METAINFO = "app/src-tauri/linux/com.omznc.garmincompanion.metainfo.xml";

const { version } = JSON.parse(readFileSync(CONF, "utf8"));
const xml = readFileSync(METAINFO, "utf8");

const releases = [...xml.matchAll(/<release\s+version="([^"]+)"\s+date="([^"]+)"/g)].map(
  ([, version, date]) => ({ version, date }),
);

const problems = [];

if (!releases.length) {
  problems.push(`${METAINFO} has no <release> entries at all`);
} else if (releases[0].version !== version) {
  problems.push(
    `${CONF} is at ${version}, but the newest <release> in ${METAINFO} is ` +
      `${releases[0].version}. Add an entry for ${version} — see RELEASING.md.`,
  );
}

for (const { version: v, date } of releases) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) {
    problems.push(`release ${v} has date="${date}", which AppStream wants as YYYY-MM-DD`);
  }
}

// Newest first is what a software centre assumes when it picks "the current
// release" off the top of the list, and the file is only ever read in order.
const ordered = [...releases].sort((a, b) => compare(b.version, a.version));
if (releases.some((r, i) => r.version !== ordered[i].version)) {
  problems.push(
    `<releases> is out of order — ${METAINFO} must list newest first, got ` +
      releases.map((r) => r.version).join(", "),
  );
}

/** Enough of semver to order releases that are all `major.minor.patch`. */
function compare(a, b) {
  const parts = (v) => v.split(".").map(Number);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  return 0;
}

if (problems.length) {
  for (const problem of problems) console.error(`error: ${problem}`);
  process.exit(1);
}

console.log(`metainfo is at ${version}, ${releases.length} releases listed`);
