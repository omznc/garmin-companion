import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { test } from "node:test";

const require = createRequire(import.meta.url);
const appJson = require("../app.json");
const makeConfig = require("../app.config.js");

// A top-level or iOS backgroundColor sets the iOS window colour. That colour
// stays light in dark mode, so only the Android key may be set.
test("only the Android window colour is set", () => {
  const config = makeConfig({ config: appJson.expo });
  assert.ok(config.android.backgroundColor);
  assert.equal(config.backgroundColor, undefined);
  assert.equal(config.ios?.backgroundColor, undefined);
});
