"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const { buildContext, loadFiles } = require("./helpers/load-extension");

function loadAxonRedact() {
  const ctx = buildContext();
  loadFiles(ctx, ["capture-redaction.js"]);
  return ctx.AxonRedact;
}

test("redactText masks a bearer token", () => {
  const { text, redactions } = loadAxonRedact().redactText("Authorization header: Bearer sk_live_abcdef0123456789");
  assert.match(text, /\[REDACTED\]/);
  assert.doesNotMatch(text, /sk_live_abcdef0123456789/);
  assert.ok(redactions.length > 0);
});

test("redactText masks an Authorization header line", () => {
  const { text } = loadAxonRedact().redactText("Authorization: Bearer abcdefghijklmnopqrstuvwxyz012345");
  assert.match(text, /^Authorization:\s*\[REDACTED\]$/m);
});

test("redactText masks a Cookie header line", () => {
  const { text } = loadAxonRedact().redactText("Cookie: session=abc123; other=xyz789");
  assert.match(text, /Cookie:\s*\[REDACTED\]/);
});

test("redactText masks credentials embedded in a URL", () => {
  const { text } = loadAxonRedact().redactText("fetched https://user:hunter2pass@example.com/path");
  assert.match(text, /https:\/\/\[REDACTED\]@example\.com\/path/);
  assert.doesNotMatch(text, /hunter2pass/);
});

test("redactText masks a secret-shaped key=value pair", () => {
  const { text } = loadAxonRedact().redactText('api_key: "abcdef0123456789"');
  assert.match(text, /api_key=\[REDACTED\]/);
});

test("redactText masks an AWS access key id", () => {
  const { text } = loadAxonRedact().redactText("AKIAABCDEFGHIJKLMNOP is the key");
  assert.match(text, /\[REDACTED_AWS_KEY\]/);
});

test("redactText masks a JWT-shaped token", () => {
  const jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
  const { text } = loadAxonRedact().redactText(`token=${jwt}`);
  assert.match(text, /\[REDACTED_JWT\]/);
  assert.doesNotMatch(text, /eyJhbGciOiJIUzI1NiJ9/);
});

test("redactText leaves ordinary non-secret prose untouched", () => {
  const input = "The quick brown fox jumps over the lazy dog. Visit https://example.com/docs for more info.";
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactText leaves a short non-secret identifier untouched", () => {
  const input = "issue #4821 was fixed in build 2026.07.10";
  const { text, redactions } = loadAxonRedact().redactText(input);
  assert.equal(text, input);
  assert.equal(redactions.length, 0);
});

test("redactUrl strips basic-auth credentials", () => {
  const url = loadAxonRedact().redactUrl("https://admin:s3cr3t@internal.example.com/dash");
  assert.equal(url, "https://[REDACTED]@internal.example.com/dash");
});

test("redactUrl leaves a plain URL untouched", () => {
  const url = loadAxonRedact().redactUrl("https://example.com/page?q=1");
  assert.equal(url, "https://example.com/page?q=1");
});
