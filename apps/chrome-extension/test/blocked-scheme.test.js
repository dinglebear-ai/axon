"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { buildContext, loadFiles } = require("./helpers/load-extension");

function loadAxonRedact() {
  const ctx = buildContext();
  loadFiles(ctx, ["src/redaction/capture-redaction.js"]);
  return ctx.AxonRedact;
}

const BLOCKED_SCHEMES = [
  "chrome://extensions",
  "chrome-extension://abcdefghijklmnop/popup.html",
  "edge://settings",
  "about:blank",
  "devtools://devtools/bundled/inspector.html",
  "view-source:https://example.com",
  "file:///etc/passwd",
  "data:text/html,<h1>hi</h1>",
  "blob:https://example.com/uuid"
];

for (const url of BLOCKED_SCHEMES) {
  test(`isBlockedCaptureUrl blocks ${url}`, () => {
    const AxonRedact = loadAxonRedact();
    assert.equal(AxonRedact.isBlockedCaptureUrl(url), true);
    assert.match(AxonRedact.blockedCaptureReason(url), /can't capture/);
  });
}

const ALLOWED_URLS = ["https://example.com/", "http://example.com/path?q=1", "https://sub.example.co.uk/a/b"];

for (const url of ALLOWED_URLS) {
  test(`isBlockedCaptureUrl allows ${url}`, () => {
    const AxonRedact = loadAxonRedact();
    assert.equal(AxonRedact.isBlockedCaptureUrl(url), false);
    assert.equal(AxonRedact.blockedCaptureReason(url), null);
  });
}

test("isBlockedCaptureUrl is case-insensitive on scheme", () => {
  const AxonRedact = loadAxonRedact();
  assert.equal(AxonRedact.isBlockedCaptureUrl("CHROME://version"), true);
});

test("blockedCaptureReason names the offending scheme", () => {
  const AxonRedact = loadAxonRedact();
  const reason = AxonRedact.blockedCaptureReason("file:///etc/passwd");
  assert.match(reason, /file:/);
});
