import assert from "node:assert/strict";
import { test } from "node:test";
import { parseManifest } from "./manifest.ts";

const hash = "a".repeat(64);

test("accepts a complete manifest and lowercases the hash", () => {
  const m = parseManifest({ version: "0.5.0", url: "https://x/a.apk", sha256: hash.toUpperCase() });
  assert.deepEqual(m, { version: "0.5.0", url: "https://x/a.apk", sha256: hash });
});

test("refuses a missing or short field", () => {
  assert.equal(parseManifest(null), null);
  assert.equal(parseManifest({ version: "0.5.0", url: "https://x/a.apk" }), null);
  assert.equal(parseManifest({ version: "", url: "https://x/a.apk", sha256: hash }), null);
  assert.equal(parseManifest({ version: "1", url: "u", sha256: "abc" }), null);
});
