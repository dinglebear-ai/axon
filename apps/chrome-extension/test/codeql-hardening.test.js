"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");

const extensionRoot = path.join(__dirname, "..");

function source(relativePath) {
  return fs.readFileSync(path.join(extensionRoot, relativePath), "utf8");
}

test("sidepanel DOM builder has no raw HTML property", () => {
  const render = source("src/sidepanel/launcher-render.js");
  assert.doesNotMatch(render, /innerHTML/);
  assert.doesNotMatch(render, /k === ["']html["']/);
});

test("plain-text error paths do not perform partial HTML sanitization", () => {
  for (const relativePath of ["src/sidepanel/launcher.js", "src/popup/popup-api.js"]) {
    const api = source(relativePath);
    assert.doesNotMatch(api, /replace\(\/<!--/);
  }
});
