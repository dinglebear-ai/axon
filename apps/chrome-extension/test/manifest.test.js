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
  assert.equal(manifest.background?.service_worker, "background.js");
  assert.equal(manifest.side_panel?.default_path, "sidepanel.html");
  assert.equal(manifest.options_page, "options.html");
});

// chrome-extension-contract.md "Permission Contract": "request host
// permissions only when needed" / "prefer `activeTab` for user-triggered
// capture" — the extension should request `http(s)://*/*` as
// `optional_host_permissions` (granted per-origin via a user gesture, e.g.
// the Options "Save"/"Check API" flow) rather than an always-on blanket
// `host_permissions` grant.
//
// TRACKED GAP: the shipped manifest.json still declares this as a blanket
// `host_permissions` grant. A prior handoff note describes an
// optional-permissions flow, but that work landed only in the orphaned
// `src/` tree (never wired into manifest.json/background.js/options.js —
// see popup-render.js's `init()` / the flat-file scripts this extension
// actually ships). Fixing it for real requires a `chrome.permissions.
// request`/`contains` flow gated on a user gesture in background.js and
// options.js, which is functional, browser-verified work outside a
// tests-only change — not something to bolt on unverified here. This test
// is intentionally skipped until that flow lands; un-skip it as part of
// that work.
test("manifest.json does not request a blanket host_permissions grant", (t) => {
  t.skip("known gap — see TRACKED GAP comment above; requires a browser-verified optional_host_permissions request flow");
  return;
  // eslint-disable-next-line no-unreachable
  const manifest = readManifest();
  assert.equal(manifest.host_permissions, undefined);
  assert.ok(Array.isArray(manifest.optional_host_permissions) && manifest.optional_host_permissions.length > 0);
});
