"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const MANIFEST_PATH = path.join(__dirname, "..", "manifest.json");

function readManifest() {
  return JSON.parse(fs.readFileSync(MANIFEST_PATH, "utf8"));
}

test("manifest.json is valid JSON with manifest_version 3", () => {
  const manifest = readManifest();
  assert.equal(manifest.manifest_version, 3);
});

test("manifest.json declares only the documented minimal permission set", () => {
  const manifest = readManifest();
  const documented = new Set(["activeTab", "clipboardWrite", "contextMenus", "notifications", "offscreen", "sidePanel", "storage", "tabs"]);
  assert.ok(Array.isArray(manifest.permissions));
  for (const permission of manifest.permissions) {
    assert.ok(documented.has(permission), `undocumented permission "${permission}" — chrome-extension-contract.md requires minimal, reviewed permissions`);
  }
  // `scripting`/`declarativeNetRequest`/`background` (host-history-shaped) must
  // never sneak in without an explicit contract update.
  assert.ok(!manifest.permissions.includes("background"));
  assert.ok(!manifest.permissions.includes("declarativeNetRequest"));
});

test("manifest.json never requests <all_urls> literally", () => {
  const manifest = readManifest();
  const hostPatterns = [...(manifest.host_permissions || []), ...(manifest.optional_host_permissions || [])];
  assert.ok(!hostPatterns.includes("<all_urls>"));
});

test("manifest.json wires the expected background/side-panel/options entry points", () => {
  const manifest = readManifest();
  assert.equal(manifest.background?.service_worker, "src/background/background.js");
  assert.equal(manifest.side_panel?.default_path, "src/sidepanel/sidepanel.html");
  assert.equal(manifest.options_page, "src/options/options.html");
});

// chrome-extension-contract.md "Permission Contract": "request host
// permissions only when needed" / "prefer `activeTab` for user-triggered
// capture" — the extension requests `http(s)://*/*` as
// `optional_host_permissions` (granted per-origin via a user gesture, e.g.
// the Options "Save"/"Check API" flow, background.js's context-menu
// actions, popup-api.js's request layer, and launcher.js's request layer —
// see host-permissions.js's `AxonHostPermissions` helper) rather than an
// always-on blanket `host_permissions` grant.
test("manifest.json does not request a blanket host_permissions grant", () => {
  const manifest = readManifest();
  assert.equal(manifest.host_permissions, undefined);
  assert.ok(Array.isArray(manifest.optional_host_permissions) && manifest.optional_host_permissions.length > 0);
});
